//! Qualification observations, never timing instrumentation or rollout approval.
use serde_json::{json, Value};

fn known_bytes(value: isize) -> Option<u64> {
    // HAL counters are backend-dependent. Zero can mean unsupported, and a
    // snapshot is not atomic across counters. Never interpret it as zero usage.
    (value > 0).then_some(value as u64)
}

#[cfg(target_os = "macos")]
fn metal_allocations(device: &wgpu::Device) -> Option<Value> {
    // SAFETY: the HAL guard keeps this exact Metal device alive. Both objc2
    // versions describe the same opaque Objective-C runtime object; borrow its
    // pointer only to send read-only NSObject/MTLDevice messages. No handle or
    // pointer escapes, and no wgpu resource state is changed.
    unsafe {
        let metal = device.as_hal::<wgpu::hal::api::Metal>()?;
        let object =
            &*(std::ptr::from_ref(&**metal.raw_device()).cast::<objc2::runtime::AnyObject>());
        let supported: bool =
            objc2::msg_send![object, respondsToSelector: objc2::sel!(currentAllocatedSize)];
        if !supported {
            return None;
        }
        let bytes: usize = objc2::msg_send![object, currentAllocatedSize];
        Some(json!({
            "allocated_bytes": bytes,
            "scope": "MTLDevice.currentAllocatedSize; may include other owners sharing this native device; not residency",
        }))
    }
}

#[cfg(not(target_os = "macos"))]
fn metal_allocations(_device: &wgpu::Device) -> Option<Value> {
    None
}

pub(super) fn snapshot(device: &wgpu::Device) -> Value {
    snapshot_for_role(device, "presenter")
}

pub(super) fn snapshot_for_role(device: &wgpu::Device, role: &str) -> Value {
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
        "role": role,
        "scope": "HAL/allocator counters cover this device; native Metal scope is reported separately; incomplete driver/process residency",
        "native_metal": metal_allocations(device),
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
        let baseline = snapshot(&device);
        let before = baseline["hal_raw"]["buffers"].as_i64().unwrap();
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
        #[cfg(target_os = "macos")]
        if adapter.get_info().backend == wgpu::Backend::Metal {
            let live_bytes = live["native_metal"]["allocated_bytes"].as_u64().unwrap();
            assert!(
                live_bytes >= 4096,
                "native Metal observes the live buffer allocation"
            );
        }
        drop(buffer);
        device.poll(wgpu::PollType::Poll).unwrap();
        assert_eq!(
            snapshot(&device)["hal_raw"]["buffers"].as_i64().unwrap(),
            before
        );
    }
}
