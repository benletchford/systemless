//! Explicit test-support diagnostics; streaming observations never approve rollout.
use super::{triangle_area, PpcQ3RasterFrame, PpcQ3RasterPrimitive};
use std::{
    cell::{Cell, RefCell},
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
    time::Duration,
};

pub(super) struct RoutingTrace {
    file: RefCell<File>,
    selection_index: Cell<u64>,
}

impl RoutingTrace {
    pub(super) fn from_environment() -> Option<Self> {
        std::env::var_os("SYSTEMLESS_QD3D_ROUTING_TRACE")
            .map(|path| Self::create(Path::new(&path)).expect("create fresh QD3D routing trace"))
    }

    fn create(path: &Path) -> io::Result<Self> {
        let trace = Self {
            file: RefCell::new(OpenOptions::new().write(true).create_new(true).open(path)?),
            selection_index: Cell::new(0),
        };
        trace.write(serde_json::json!({
            "event":"metadata", "schema":1,
            "scope":"test-support routing and synchronous render requests; excludes CPU fallback execution and guest-memory commit; request elapsed includes first-use initialization, CPU preparation, submission and readback; logging perturbs execution; not hardware GPU time, physical frame pacing or production performance qualification"
        }));
        Ok(trace)
    }

    fn write(&self, value: serde_json::Value) {
        writeln!(self.file.borrow_mut(), "{value}").expect("write QD3D routing trace");
    }

    pub(super) fn work_estimate(&self, cpu_work: u64, tiled_work: u64, minimum_cpu_work: u64) {
        self.write(serde_json::json!({
            "event":"work_estimate",
            "selection_index":self.selection_index.get().checked_add(1).expect("QD3D trace sequence overflow"),
            "cpu_work":cpu_work, "tiled_work":tiled_work, "minimum_cpu_work":minimum_cpu_work,
            "maximum_tiled_work":super::Qd3dWorkEstimate::MAX_GPU_WORK,
            "maximum_tiled_to_cpu_ratio":super::Qd3dWorkEstimate::MAX_GPU_TO_CPU_RATIO,
            "scope":"production adaptive work estimates, not measured CPU or GPU time"
        }));
    }

    pub(super) fn selection(&self, frame: &PpcQ3RasterFrame, selected: bool) {
        let index = self
            .selection_index
            .get()
            .checked_add(1)
            .expect("QD3D trace sequence overflow");
        self.selection_index.set(index);
        let mut primitives = 0u64;
        let mut triangles = 0u64;
        let mut degenerates = 0u64;
        let mut maximum_lighting_work = 1u64;
        for draw in &frame.draws {
            maximum_lighting_work = maximum_lighting_work
                .max(1 + draw.lighting.as_ref().map_or(0, |l| l.lights.len()) as u64);
            for vertices in draw.vertices.chunks_exact(3) {
                primitives += 1;
                if draw.primitive == PpcQ3RasterPrimitive::Triangles {
                    triangles += 1;
                    degenerates += u64::from(triangle_area(vertices) == 0.);
                }
            }
        }
        let legacy_work = u64::from(frame.width)
            .saturating_mul(u64::from(frame.height))
            .saturating_mul(primitives)
            .saturating_mul(maximum_lighting_work);
        self.write(serde_json::json!({
            "event":"selection", "selection_index":index, "gpu_selected":selected,
            "width":frame.width, "height":frame.height, "draws":frame.draws.len(),
            "primitives":primitives, "triangles":triangles, "degenerate_triangles":degenerates,
            "texture_count":frame.textures.len(), "legacy_full_frame_work":legacy_work,
            "legacy_work_scope":"old full-frame estimate with maximum lighting count; not current tiled work"
        }));
    }

    pub(super) fn render_result(
        &self,
        result: &Result<Vec<u16>, String>,
        elapsed: Duration,
        indexed: bool,
    ) {
        self.write(serde_json::json!({
            "event":"render_result", "selection_index":self.selection_index.get(),
            "ok":result.is_ok(), "error":result.as_ref().err(),
            "pixels":result.as_ref().ok().map(Vec::len), "indexed":indexed,
            "request_elapsed_us":elapsed.as_secs_f64()*1e6
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_estimates_refer_to_the_next_selection_without_advancing_it() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("work.jsonl");
        let trace = RoutingTrace::create(&path).unwrap();
        trace.work_estimate(123, 456, 200_000);
        assert_eq!(trace.selection_index.get(), 0);
        let text = std::fs::read_to_string(path).unwrap();
        let row: serde_json::Value = serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
        assert_eq!(row["selection_index"], 1);
        assert_eq!(row["cpu_work"], 123);
        assert_eq!(row["tiled_work"], 456);
        assert_eq!(row["minimum_cpu_work"], 200_000);
        assert_eq!(row["maximum_tiled_work"], 512_000_000);
    }

    #[test]
    fn records_real_packet_work_and_degenerate_counts() {
        use systemless::loader::ppc::{PpcQ3GpuDraw, PpcQ3GpuFrame, PpcQ3GpuVertex};
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("packet.jsonl");
        let trace = RoutingTrace::create(&path).unwrap();
        let vertex = PpcQ3GpuVertex {
            screen_x: 5.,
            screen_y: 5.,
            depth: 0.5,
            reciprocal_w: 1.,
            uv: [0.; 2],
            color: [1.; 4],
        };
        let frame = PpcQ3RasterFrame::from(&PpcQ3GpuFrame {
            width: 640,
            height: 480,
            viewport: [0, 0, 639, 479],
            clear_color: None,
            textures: vec![],
            draws: vec![PpcQ3GpuDraw {
                texture: None,
                blend: false,
                write_depth: true,
                vertices: vec![vertex; 1792 * 3],
            }],
        });
        trace.selection(&frame, true);
        let text = std::fs::read_to_string(path).unwrap();
        let packet: serde_json::Value = serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
        assert_eq!(packet["primitives"], 1792);
        assert_eq!(packet["triangles"], 1792);
        assert_eq!(packet["degenerate_triangles"], 1792);
        assert_eq!(packet["legacy_full_frame_work"], 550_502_400u64);
    }

    #[test]
    fn streams_selections_and_errors_without_overwriting_prior_evidence() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("routing.jsonl");
        let trace = RoutingTrace::create(&path).unwrap();
        let frame = PpcQ3RasterFrame {
            width: 640,
            height: 480,
            viewport: [0, 0, 639, 479],
            clear_color: None,
            textures: vec![],
            draws: vec![],
        };
        trace.selection(&frame, false);
        trace.selection(&frame, true);
        trace.render_result(
            &Err("injected render failure".into()),
            Duration::from_micros(42),
            false,
        );
        trace.render_result(&Ok(vec![0; 4]), Duration::from_micros(43), true);
        assert!(RoutingTrace::create(&path).is_err());
        let rows: Vec<serde_json::Value> = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0]["schema"], 1);
        assert_eq!(rows[1]["gpu_selected"], false);
        assert_eq!(rows[2]["selection_index"], 2);
        assert_eq!(rows[3]["selection_index"], 2);
        assert_eq!(rows[3]["ok"], false);
        assert_eq!(rows[3]["error"], "injected render failure");
        assert_eq!(rows[3]["request_elapsed_us"], 42.);
        assert_eq!(rows[4]["pixels"], 4);
        assert_eq!(rows[4]["indexed"], true);
    }
}
