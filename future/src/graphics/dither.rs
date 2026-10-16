//! GPU error diffusion follows the preserved Systemless GWorld conversion policy.
//! Native QuickDraw pixel parity is not established by that comparison.
use super::*;
pub(super) fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("RGB error diffusion"),
        source: wgpu::ShaderSource::Wgsl(include_str!("dither.wgsl").into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("deterministic Floyd Steinberg"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}
impl Graphics {
    pub(crate) fn execute_dither_rgb(
        &self,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
        rgb: &[[u16; 3]],
        target: Option<IndexedTarget<'_>>,
    ) -> Result<()> {
        self.check_error()?;
        let Draw::Blit {
            mask: None,
            rect,
            clip,
            source,
            width,
            height,
            foreground,
            background,
            invert,
            mode,
        } = draw
        else {
            return Err(Error::Graphics(
                "dithering requires a region-clipped RGB blit".into(),
            ));
        };
        let w = rect
            .right
            .checked_sub(rect.left)
            .filter(|v| *v > 0 && *v <= 4096)
            .ok_or_else(|| Error::Graphics("dither destination width".into()))?
            as u32;
        let h = rect
            .bottom
            .checked_sub(rect.top)
            .filter(|v| *v > 0 && *v <= 4096)
            .ok_or_else(|| Error::Graphics("dither destination height".into()))?
            as u32;
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || rgb.len() != width as usize * height as usize
            || source.right <= source.left
            || source.bottom <= source.top
            || source.right - source.left > 4096
            || source.bottom - source.top > 4096
            || [source.left, source.top, source.right, source.bottom]
                .iter()
                .any(|v| !(-32768..=32767).contains(v))
        {
            return Err(Error::Graphics("invalid dither source".into()));
        }
        let maximum = self.device.limits().max_storage_buffer_binding_size;
        if u64::from(w) * u64::from(h) * 12 > maximum || rgb.len() as u64 * 12 > maximum {
            return Err(Error::Graphics(
                "dither RGB storage exceeds device binding limit".into(),
            ));
        }
        let mono = std::array::from_fn(|i| if i == 0 { [65535u16; 3] } else { [0; 3] });
        let count = target.as_ref().map_or(2, |t| 1u32 << t.depth);
        let colors = target.as_ref().map_or(&mono, |t| t.rgb);
        let eligible = target.as_ref().map(|t| t.matchable);
        if !(0..count as usize).any(|i| eligible.is_none_or(|e| e[i])) {
            return Err(Error::Graphics("no matchable dither colors".into()));
        }
        if u64::from(w) * u64::from(h) * u64::from(count) > 128_000_000 {
            return Err(Error::Graphics(
                "dither color matching work limit exceeded".into(),
            ));
        }
        let mut p = vec![
            w,
            h,
            width,
            height,
            source.left as u32,
            source.top as u32,
            source.right as u32,
            source.bottom as u32,
            count,
        ];
        for i in 0..count as usize {
            p.extend(colors[i].map(u32::from));
            p.push(u32::from(eligible.is_none_or(|e| e[i])));
        }
        let source_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("unquantized RGB source"),
                contents: &words_bytes(rgb.iter().flatten().map(|&v| u32::from(v))),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let params = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("diffusion geometry and palette"),
                contents: &words_bytes(p),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU dithered RGB"),
            size: u64::from(w) * u64::from(h) * 12,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let errors = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("two GPU diffusion rows"),
            size: u64::from(w + 2) * 2 * 3 * 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.dither_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: source_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: errors.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.dither_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        self.queue.submit([encoder.finish()]);
        let draw = Draw::Blit {
            mask: None,
            rect,
            clip,
            source: Rect::new(0, 0, w as i32, h as i32),
            width: w,
            height: h,
            foreground,
            background,
            invert,
            mode,
        };
        self.execute_source_input(
            memory,
            draw,
            clips,
            Some(CopySource::GpuRgb(&output)),
            None,
            target,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference(
        rgb: &[[u16; 3]],
        size: (usize, usize),
        source: Rect,
        dest: (usize, usize),
        palette: &[[u16; 3]],
    ) -> Vec<[u16; 3]> {
        let (w, h) = dest;
        let mut result = Vec::new();
        let mut current = vec![[0i32; 3]; w + 2];
        let mut next = current.clone();
        for y in 0..h {
            for x in 0..w {
                let sx = source.left as usize + x * (source.right - source.left) as usize / w;
                let sy = source.top as usize + y * (source.bottom - source.top) as usize / h;
                assert!(sx < size.0 && sy < size.1);
                let adjusted = std::array::from_fn::<_, 3, _>(|c| {
                    (i32::from(rgb[sy * size.0 + sx][c]) + current[x + 1][c]).clamp(0, 65535) as u16
                });
                let chosen = *palette
                    .iter()
                    .min_by_key(|color| {
                        adjusted
                            .iter()
                            .zip(color.iter())
                            .map(|(&a, &b)| i64::from(a) - i64::from(b))
                            .map(|d| d * d)
                            .sum::<i64>()
                    })
                    .unwrap();
                result.push(chosen);
                for c in 0..3 {
                    let error = i32::from(adjusted[c]) - i32::from(chosen[c]);
                    current[x + 2][c] += error * 7 / 16;
                    next[x][c] += error * 3 / 16;
                    next[x + 1][c] += error * 5 / 16;
                    next[x + 2][c] += error / 16;
                }
            }
            std::mem::swap(&mut current, &mut next);
            next.fill([0; 3]);
        }
        result
    }
    #[test]
    #[ignore = "requires a real wgpu adapter"]
    fn gpu_diffusion_matches_reference_after_scaling_with_clips_and_all_mono_transfers() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let mut gpu =
            pollster::block_on(Graphics::new(&instance, None, 32, 8, GuestPtr(0x1000))).unwrap();
        gpu.palette[0] = [255; 4];
        gpu.palette[1] = [0, 0, 0, 255];
        let mut memory = Memory::default();
        memory
            .map(gpu.base, vec![0; 256], crate::memory::Permissions::RW)
            .unwrap();
        let rgb: Vec<_> = (0..64u16)
            .map(|i| [12000 + i * 650, 20000 + i * 350, 30000 + i * 100])
            .collect();
        let source = Rect::new(2, 0, 14, 4);
        let bounds = Rect::new(0, 0, 32, 8);
        let rect = Rect::new(-3, 0, 29, 8);
        let quantized = reference(&rgb, (16, 4), source, (32, 8), &[[65535; 3], [0; 3]]);
        assert!(quantized.contains(&[65535; 3]) && quantized.contains(&[0; 3]));
        let clips = [Rect::new(0, 0, 12, 8), Rect::new(20, 0, 32, 8)];
        let initial: Vec<u8> = (0..256).map(|i| (i % 2) as u8).collect();
        for mode in [Transfer::Copy, Transfer::Or, Transfer::Xor, Transfer::Bic] {
            for invert in [false, true] {
                memory.write(gpu.base, &initial).unwrap();
                let draw = Draw::Blit {
                    mask: None,
                    rect,
                    clip: bounds,
                    source,
                    width: 16,
                    height: 4,
                    foreground: 1,
                    background: 0,
                    invert,
                    mode,
                };
                gpu.execute_dither_rgb(&mut memory, draw, Some(&clips), &rgb, None)
                    .unwrap();
                let mut expected = initial.clone();
                for y in 0..8 {
                    for x in 0..29 {
                        if (12..20).contains(&x) {
                            continue;
                        }
                        let bit = u8::from(quantized[y * 32 + x + 3] == [0; 3]) ^ u8::from(invert);
                        let dst = initial[y * 32 + x];
                        expected[y * 32 + x] = match mode {
                            Transfer::Copy => bit,
                            Transfer::Or => dst | bit,
                            Transfer::Xor => dst ^ bit,
                            Transfer::Bic => dst & !bit,
                        };
                    }
                }
                assert_eq!(memory.read(gpu.base, 256).unwrap(), expected);
            }
        }
        let before = memory.read(gpu.base, 256).unwrap().to_vec();
        let draw = Draw::Blit {
            mask: None,
            rect,
            clip: bounds,
            source,
            width: 16,
            height: 4,
            foreground: 1,
            background: 0,
            invert: false,
            mode: Transfer::Copy,
        };
        assert!(gpu
            .execute_dither_rgb(&mut memory, draw, None, &rgb[..63], None)
            .is_err());
        assert_eq!(memory.read(gpu.base, 256).unwrap(), before);
    }
    #[test]
    #[ignore = "requires a real wgpu adapter"]
    fn gpu_diffusion_uses_indexed_palette_eligibility_and_differs_from_nearest_copy() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let gpu =
            pollster::block_on(Graphics::new(&instance, None, 32, 8, GuestPtr(0x1000))).unwrap();
        let mut memory = Memory::default();
        memory
            .map(gpu.base, vec![0; 256], crate::memory::Permissions::RW)
            .unwrap();
        let mut palette = [[0; 3]; 256];
        palette[1] = [65535; 3];
        palette[2] = [65535, 0, 0];
        palette[3] = [0, 65535, 0];
        let mut eligible = [false; 256];
        eligible[..4].fill(true);
        let rgb = vec![[32768; 3]; 256];
        let bounds = Rect::new(0, 0, 32, 8);
        let draw = Draw::Blit {
            mask: None,
            rect: bounds,
            clip: bounds,
            source: bounds,
            width: 32,
            height: 8,
            foreground: 0,
            background: 1,
            invert: false,
            mode: Transfer::Copy,
        };
        let reference = reference(&rgb, (32, 8), bounds, (32, 8), &palette[..4]);
        for depth in [2, 4, 8] {
            gpu.execute_dither_rgb(
                &mut memory,
                draw,
                None,
                &rgb,
                Some(IndexedTarget {
                    depth,
                    rgb: &palette,
                    matchable: &eligible,
                    direct_source: false,
                    neutral_copy: false,
                }),
            )
            .unwrap();
            let expected: Vec<u8> = reference
                .iter()
                .map(|c| palette[..4].iter().position(|p| p == c).unwrap() as u8)
                .collect();
            assert_eq!(memory.read(gpu.base, 256).unwrap(), expected);
        }
        let dithered = memory.read(gpu.base, 256).unwrap().to_vec();
        gpu.execute_rgb(
            &mut memory,
            draw,
            None,
            &rgb,
            Some(IndexedTarget {
                depth: 8,
                rgb: &palette,
                matchable: &eligible,
                direct_source: false,
                neutral_copy: false,
            }),
        )
        .unwrap();
        assert_ne!(memory.read(gpu.base, 256).unwrap(), dithered);
        eligible[1] = false;
        gpu.execute_dither_rgb(
            &mut memory,
            draw,
            None,
            &rgb,
            Some(IndexedTarget {
                depth: 2,
                rgb: &palette,
                matchable: &eligible,
                direct_source: false,
                neutral_copy: false,
            }),
        )
        .unwrap();
        assert!(memory.read(gpu.base, 256).unwrap().iter().all(|i| *i != 1));
    }
}
