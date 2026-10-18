//! Qualification observations, never timing instrumentation or rollout approval.
use serde_json::{json, Value};

fn known_bytes(value: isize) -> Option<u64> {
    // HAL counters are backend-dependent. Zero can mean unsupported, and a
    // snapshot is not atomic across counters. Never interpret it as zero usage.
    (value > 0).then_some(value as u64)
}

pub(super) fn snapshot(device: &wgpu::Device) -> Value {
    let info = device.adapter_info();
    let counters = device.get_internal_counters().hal;
    let allocator = device.generate_allocator_report().map(|report| {
        json!({
            "allocated_bytes": report.total_allocated_bytes,
            "reserved_bytes": report.total_reserved_bytes,
            "allocations": report.allocations.len(), "blocks": report.blocks.len(),
        })
    });
    json!({
        "scope": "presenter device only; excludes guest compute devices and complete driver/process residency",
        "adapter": {"name": info.name, "backend": format!("{:?}", info.backend),
                    "device_type": format!("{:?}", info.device_type),
                    "vendor": info.vendor, "device": info.device,
                    "driver": info.driver, "driver_info": info.driver_info},
        "hal_counters_enabled": true,
        "hal_counters_are_backend_dependent": true,
        "hal_raw": {
            "buffers": counters.buffers.read(), "textures": counters.textures.read(),
            "texture_views": counters.texture_views.read(), "bind_groups": counters.bind_groups.read(),
            "bind_group_layouts": counters.bind_group_layouts.read(),
            "render_pipelines": counters.render_pipelines.read(),
            "compute_pipelines": counters.compute_pipelines.read(),
            "pipeline_layouts": counters.pipeline_layouts.read(), "samplers": counters.samplers.read(),
            "command_encoders": counters.command_encoders.read(), "shader_modules": counters.shader_modules.read(),
            "query_sets": counters.query_sets.read(), "fences": counters.fences.read(),
            "memory_allocations": counters.memory_allocations.read(),
        },
        "hal_memory_bytes": {
            "buffers": known_bytes(counters.buffer_memory.read()),
            "textures": known_bytes(counters.texture_memory.read()),
            "zero_is_unknown": true,
        },
        "allocator": allocator,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_memory_accounting_is_not_reported_as_zero_usage() {
        assert_eq!(known_bytes(0), None);
        assert_eq!(known_bytes(-1), None);
        assert_eq!(known_bytes(4096), Some(4096));
    }

    #[test]
    fn wgpu_allocation_snapshot_tracks_unsubmitted_buffer_ownership() {
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
        let (device, _queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let before = snapshot(&device)["hal_raw"]["buffers"].as_i64().unwrap();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("qualification ownership observation"),
            size: 4096,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let live = snapshot(&device);
        assert_eq!(
            live["hal_raw"]["buffers"].as_i64().unwrap(),
            before + 1,
            "native qualification needs working HAL buffer counters"
        );
        assert_eq!(live["hal_counters_enabled"], true);
        drop(buffer);
        device.poll(wgpu::PollType::Poll).unwrap();
        assert_eq!(
            snapshot(&device)["hal_raw"]["buffers"].as_i64().unwrap(),
            before
        );
    }
}
