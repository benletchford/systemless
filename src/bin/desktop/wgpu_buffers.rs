//! Native readback waits for synchronous guest-visible GPU operations. Scratch
//! resources are shared with the browser compute worker.
pub(super) use systemless::gpu_compute::word_bytes;
#[cfg(test)]
use systemless::gpu_compute::{BindingRole, ScratchBuffers};

const READBACK_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);
const SOFTWARE_READBACK_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);
const METAL_SHORT_WAIT: std::time::Duration = std::time::Duration::from_micros(50);
const METAL_SHORT_BUDGET: std::time::Duration = std::time::Duration::from_millis(4);

/// Software adapters execute the same shader work on shared host CPUs and
/// can legitimately take much longer than physical GPUs. Keep that path finite
/// without treating software correctness runs as physical performance evidence.
pub(super) fn readback_budget(device: &wgpu::Device) -> std::time::Duration {
    budget_for_type(device.adapter_info().device_type)
}

fn budget_for_type(kind: wgpu::DeviceType) -> std::time::Duration {
    if kind == wgpu::DeviceType::Cpu {
        SOFTWARE_READBACK_BUDGET
    } else {
        READBACK_BUDGET
    }
}

/// Sleep in the backend while allowing Metal to observe completion even when
/// its separately scheduled notification is late. Every wait has a deadline;
/// a stalled compute queue must return control to guest CPU fallback.
pub(super) fn wait_for_readback(
    device: &wgpu::Device,
    budget: std::time::Duration,
) -> Result<wgpu::PollStatus, wgpu::PollError> {
    let start = std::time::Instant::now();
    #[cfg(target_os = "macos")]
    // SAFETY: inspect only the backend type, retaining no HAL resource/guard.
    let metal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }.is_some();
    #[cfg(not(target_os = "macos"))]
    let metal = false;
    wait_with_budget(
        budget,
        metal,
        || start.elapsed(),
        |timeout| {
            device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(timeout),
            })
        },
    )
}

fn wait_with_budget(
    budget: std::time::Duration,
    metal: bool,
    mut elapsed: impl FnMut() -> std::time::Duration,
    mut poll: impl FnMut(std::time::Duration) -> Result<wgpu::PollStatus, wgpu::PollError>,
) -> Result<wgpu::PollStatus, wgpu::PollError> {
    if metal {
        while elapsed() < METAL_SHORT_BUDGET {
            match poll(METAL_SHORT_WAIT) {
                Err(wgpu::PollError::Timeout) => {}
                result => return result,
            }
        }
    }
    poll(budget.saturating_sub(elapsed()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, time::Duration};

    #[test]
    fn wgpu_cached_bindings_follow_pipeline_and_growing_scratch() {
        let instance = wgpu::Instance::default();
        let adapter = match pollster::block_on(instance.request_adapter(&Default::default())) {
            Ok(adapter) => adapter,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let pipelines = [1, 2].map(|value| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("binding cache oracle"),
                source: wgpu::ShaderSource::Wgsl(format!(
                    "@group(0) @binding(0) var<storage, read_write> output: array<u32>; \
                     @compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{ output[id.x] = {value}u; }}"
                ).into()),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None, layout: None, module: &shader, entry_point: Some("main"),
                compilation_options: Default::default(), cache: None,
            })
        });
        let scratch = ScratchBuffers::default();
        // Reuse, change pipeline, then grow the allocation. Each result must
        // come from the current pipeline and current output buffer.
        let mut groups = Vec::new();
        for (pipeline, count, expected) in [
            (&pipelines[0], 1, 1),
            (&pipelines[1], 1, 2),
            (&pipelines[0], 1, 1),
            (&pipelines[1], 2, 2),
        ] {
            let size = count * 4;
            let output = scratch.get(
                &device,
                "cache oracle output",
                size,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            );
            let group = scratch.bind_group(&device, BindingRole::Bitmap, pipeline, &[(0, &output)]);
            groups.push(group.clone());
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.dispatch_workgroups(count as u32, 1, 1);
            }
            encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    tx.send(result).unwrap();
                });
            wait_for_readback(&device, readback_budget(&device)).unwrap();
            rx.recv().unwrap().unwrap();
            let bytes = readback.slice(..).get_mapped_range().unwrap();
            assert_eq!(
                bytes
                    .chunks_exact(4)
                    .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                    .collect::<Vec<_>>(),
                vec![expected; count as usize]
            );
            drop(bytes);
            readback.unmap();
        }
        assert_eq!(
            groups[0], groups[2],
            "pipeline alternation must reuse bindings"
        );
        assert_ne!(groups[1], groups[3], "grown scratch must replace bindings");
    }

    #[test]
    fn wgpu_software_readback_has_a_longer_finite_budget() {
        for kind in [
            wgpu::DeviceType::IntegratedGpu,
            wgpu::DeviceType::DiscreteGpu,
            wgpu::DeviceType::VirtualGpu,
            wgpu::DeviceType::Other,
        ] {
            assert_eq!(budget_for_type(kind), READBACK_BUDGET);
        }
        let budget = budget_for_type(wgpu::DeviceType::Cpu);
        assert_eq!(budget, std::time::Duration::from_secs(60));
        let result = wait_with_budget(
            budget,
            false,
            || Duration::ZERO,
            |timeout| {
                assert_eq!(timeout, budget);
                Err(wgpu::PollError::Timeout)
            },
        );
        assert!(matches!(result, Err(wgpu::PollError::Timeout)));
    }

    #[test]
    fn wgpu_readback_wait_preserves_short_metal_completion_and_real_errors() {
        let elapsed = Cell::new(Duration::ZERO);
        let mut calls = 0;
        let result = wait_with_budget(
            READBACK_BUDGET,
            true,
            || elapsed.get(),
            |timeout| {
                assert_eq!(timeout, METAL_SHORT_WAIT);
                calls += 1;
                elapsed.set(elapsed.get() + timeout);
                if calls == 1 {
                    Err(wgpu::PollError::Timeout)
                } else {
                    Ok(wgpu::PollStatus::WaitSucceeded)
                }
            },
        );
        assert!(result.unwrap().wait_finished());
        assert_eq!(calls, 2);
        let error = wait_with_budget(
            READBACK_BUDGET,
            true,
            || Duration::ZERO,
            |_| Err(wgpu::PollError::WrongSubmissionIndex(1, 0)),
        );
        assert!(matches!(
            error,
            Err(wgpu::PollError::WrongSubmissionIndex(1, 0))
        ));
    }

    #[test]
    fn wgpu_readback_wait_exhausts_one_budget_instead_of_falling_through_to_infinity() {
        let elapsed = Cell::new(Duration::ZERO);
        let mut waits = Vec::new();
        let error = wait_with_budget(
            READBACK_BUDGET,
            true,
            || elapsed.get(),
            |timeout| {
                waits.push(timeout);
                // Model slow wakeups during the initial short waits.
                elapsed.set(elapsed.get() + timeout.max(Duration::from_millis(1)));
                Err(wgpu::PollError::Timeout)
            },
        );
        assert!(matches!(error, Err(wgpu::PollError::Timeout)));
        assert_eq!(&waits[..4], &[METAL_SHORT_WAIT; 4]);
        assert_eq!(waits.len(), 5);
        assert_eq!(waits[4], READBACK_BUDGET - Duration::from_millis(4));
        assert_eq!(elapsed.get(), READBACK_BUDGET);
    }

    #[test]
    fn wgpu_readback_wait_is_bounded_on_other_backends_and_after_a_late_wakeup() {
        let error = wait_with_budget(
            READBACK_BUDGET,
            false,
            || Duration::ZERO,
            |timeout| {
                assert_eq!(timeout, READBACK_BUDGET);
                Err(wgpu::PollError::Timeout)
            },
        );
        assert!(matches!(error, Err(wgpu::PollError::Timeout)));
        let error = wait_with_budget(
            READBACK_BUDGET,
            true,
            || READBACK_BUDGET + Duration::from_secs(1),
            |timeout| {
                assert_eq!(timeout, Duration::ZERO);
                Err(wgpu::PollError::Timeout)
            },
        );
        assert!(matches!(error, Err(wgpu::PollError::Timeout)));
    }
}
