//! Windowless qualification of the real worker-to-native-GPUI delivery path.
use super::{Args, Command, Demo, Updates};
use gpui_kit::{AppContext, Context};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

const TARGET: usize = 120;

fn record(output: &Path, demo: &Demo, count: usize, elapsed: Duration, passed: bool) {
    let passed = passed && demo.image.is_some() && demo.status.ends_with("Running");
    let result = serde_json::json!({
        "passed": passed,
        "delivered_updates": count,
        "target_updates": TARGET,
        "elapsed_ms": elapsed.as_secs_f64() * 1000.,
        "status": demo.status,
        "guest_dimensions": [demo.width, demo.height],
        "has_frame": demo.image.is_some(),
        "scope": "Real guest worker, publication mailbox, native macOS main-thread dispatcher and Demo::apply_guest_update. No window, rendered frame, display presentation, physical input, host audio or persistence qualification. Elapsed time includes startup and guest pacing; it is not delivery latency or FPS."
    });
    std::fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
}

pub(super) fn run(args: Args, output: PathBuf) {
    // An early native-loop exit must never reuse a previous successful result.
    std::fs::write(
        &output,
        b"{\"passed\":false,\"status\":\"probe started\"}\n",
    )
    .unwrap();
    let (commands, receiver) = mpsc::channel();
    let updates = Arc::new(Updates::default());
    let worker_updates = updates.clone();
    let mut worker = Some(std::thread::spawn(move || {
        super::run_guest(args, receiver, worker_updates, false)
    }));
    let shutdown = commands.clone();
    let result_path = output.clone();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.on_app_quit(move |_| {
                let _ = shutdown.send(Command::Shutdown);
                if let Some(worker) = worker.take() {
                    worker.join().expect("native delivery worker shutdown");
                }
                async {}
            })
            .detach();
            let view = cx.new(|cx: &mut Context<Demo>| Demo::new(commands, updates, cx));
            let count = Rc::new(Cell::new(0));
            let observed = count.clone();
            let started = Instant::now();
            let observer_output = output.clone();
            cx.observe(&view, move |view, cx| {
                let demo = view.read(cx);
                if demo.image.is_none() {
                    return;
                }
                let delivered = observed.get() + 1;
                observed.set(delivered);
                if delivered == TARGET {
                    record(&observer_output, demo, delivered, started.elapsed(), true);
                    cx.quit();
                }
            })
            .detach();
            // Retain the entity while its foreground task awaits publications.
            // This timer only bounds a failed probe; it never consumes updates.
            let executor = cx.background_executor().clone();
            cx.spawn(async move |cx| {
                executor.timer(Duration::from_secs(30)).await;
                cx.update(|cx| {
                    record(
                        &output,
                        view.read(cx),
                        count.get(),
                        started.elapsed(),
                        false,
                    );
                    cx.quit();
                });
            })
            .detach();
        });
    let result: serde_json::Value =
        serde_json::from_slice(&std::fs::read(result_path).unwrap()).unwrap();
    assert_eq!(
        result["passed"], true,
        "native GPUI delivery probe timed out"
    );
}
