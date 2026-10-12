//! Offscreen native accessibility-provider qualification, without activation.
//! This module is bound to the capture frontend only during qualification.

use super::{Args, Command, Demo, Updates};
use gpui_kit::{AppContext, Bounds, Context, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use objc2::{msg_send, sel, ClassType, runtime::AnyObject};
use objc2_app_kit::NSApplication;
use objc2_foundation::{MainThreadMarker, NSString};
use std::{path::PathBuf, sync::{Arc, mpsc}, time::Duration};

const TITLE: &str = "Systemless native accessibility qualification";

/// Query only our own hidden view. AccessKit's native subclass activates the
/// tree through this selector; no desktop focus or external AX permission is used.
fn native_children() -> Option<usize> {
    let main = MainThreadMarker::new()?;
    let app = NSApplication::sharedApplication(main);
    let windows = app.windows();
    let window = (0..windows.count()).filter_map(|index| windows.get(index))
        .find(|window| window.title().to_string() == TITLE)?;
    let view = window.contentView()?;
    // NSView implements NSAccessibility. The autoreleased array stays alive
    // within this main-thread callback; no pointer is retained across a yield.
    let children: *mut AnyObject = unsafe { msg_send![&*view, accessibilityChildren] };
    if children.is_null() { return Some(0); }
    Some(unsafe { msg_send![children, count] })
}

/// Read bounded descendants from our provider after publishing the frame.
/// All autoreleased objects are consumed synchronously on the main thread.
fn native_text() -> Vec<String> {
    let Some(main) = MainThreadMarker::new() else { return Vec::new(); };
    let windows = NSApplication::sharedApplication(main).windows();
    let Some(window) = (0..windows.count()).filter_map(|index| windows.get(index))
        .find(|window| window.title().to_string() == TITLE) else { return Vec::new(); };
    let Some(view) = window.contentView() else { return Vec::new(); };
    let mut pending = vec![(&*view as *const _ as *mut AnyObject, 0usize)];
    let mut visited = std::collections::HashSet::new();
    let mut text = Vec::new();
    while let Some((node, depth)) = pending.pop() {
        if node.is_null() || depth > 32 || visited.len() >= 256 || !visited.insert(node as usize) { continue; }
        unsafe {
            for selector in [sel!(accessibilityTitle), sel!(accessibilityValue)] {
                let responds: bool = msg_send![node, respondsToSelector: selector];
                if !responds { continue; }
                let value: *mut AnyObject = if selector == sel!(accessibilityTitle) {
                    msg_send![node, accessibilityTitle]
                } else { msg_send![node, accessibilityValue] };
                if value.is_null() { continue; }
                let is_string: bool = msg_send![value, isKindOfClass: NSString::class()];
                if is_string { text.push((&*(value as *const NSString)).to_string()); }
            }
            let responds: bool = msg_send![node, respondsToSelector: sel!(accessibilityChildren)];
            if !responds { continue; }
            let children: *mut AnyObject = msg_send![node, accessibilityChildren];
            if children.is_null() { continue; }
            let count: usize = msg_send![children, count];
            for index in 0..count.min(256) {
                let child: *mut AnyObject = msg_send![children, objectAtIndex: index];
                pending.push((child, depth + 1));
            }
        }
    }
    text
}

pub(super) fn run(args: Args, output: PathBuf) {
    std::fs::write(&output, b"{\"passed\":false,\"status\":\"probe started\"}\n").unwrap();
    let (commands, receiver) = mpsc::channel();
    let updates = Arc::new(Updates::default());
    let worker_updates = updates.clone();
    let mut worker = Some(std::thread::spawn(move || {
        super::run_guest(args, receiver, worker_updates, false)
    }));
    let shutdown = commands.clone();
    let result_path = output.clone();
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        cx.on_app_quit(move |_| {
            let _ = shutdown.send(Command::Shutdown);
            if let Some(worker) = worker.take() { worker.join().expect("accessibility worker shutdown"); }
            async {}
        }).detach();
        let mut view = None;
        let window = cx.open_window(WindowOptions {
            focus: false,
            show: false,
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(800.), px(600.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some(TITLE.into()), ..Default::default() }),
            ..Default::default()
        }, |_, cx| {
            let entity = cx.new(|cx: &mut Context<Demo>| Demo::new(commands, updates, cx));
            view = Some(entity.clone());
            entity
        }).unwrap();
        let view = view.unwrap();
        let window: gpui_kit::AnyWindowHandle = window.into();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx| {
            for attempt in 1..=300 {
                executor.timer(Duration::from_millis(100)).await;
                let result = window.update(cx, |_, window, cx| {
                    let initially_active = window.is_a11y_active();
                    let native_child_count = native_children();
                    window.draw(cx).clear(cx);
                    let native_text = native_text();
                    let tree = window.debug_a11y_tree_json()
                        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok());
                    let demo = view.read(cx);
                    let guest_titles: Vec<_> = demo.windows.iter()
                        .filter(|frame| frame.window.visible && !frame.window.title.is_empty())
                        .map(|frame| frame.window.title.clone()).collect();
                    let tree_text = tree.as_ref().map(serde_json::Value::to_string).unwrap_or_default();
                    let guest_title_present = guest_titles.iter().any(|title|
                        tree_text.contains(&serde_json::to_string(title).unwrap()));
                    let native_guest_title_present = guest_titles.iter().any(|title| native_text.contains(title));
                    let node_count = tree.as_ref().and_then(|tree| tree.get("nodes"))
                        .and_then(serde_json::Value::as_object).map_or(0, |nodes| nodes.len());
                    serde_json::json!({
                        "passed": window.is_a11y_active() && native_child_count.is_some_and(|count| count > 0)
                            && node_count > 1 && guest_title_present && native_guest_title_present && demo.image.is_some()
                            && demo.status.ends_with("Running"),
                        "attempt": attempt,
                        "initially_active": initially_active,
                        "a11y_active": window.is_a11y_active(),
                        "native_child_count": native_child_count,
                        "node_count": node_count,
                        "guest_titles": guest_titles,
                        "guest_title_present": guest_title_present,
                        "native_guest_title_present": native_guest_title_present,
                        "native_text": native_text,
                        "status": demo.status,
                        "tree": tree,
                        "scope": "Real native GPUI window, guest worker and native NSAccessibility provider query on the app's own hidden view. No activation or visible presentation, external assistive-tool request/action delivery, physical input, audio or persistence qualification."
                    })
                }).expect("native accessibility window remains alive");
                std::fs::write(&output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
                if result["passed"] == true {
                    cx.update(|cx| cx.quit());
                    return;
                }
            }
            cx.update(|cx| cx.quit());
        }).detach();
    });
    let result: serde_json::Value = serde_json::from_slice(&std::fs::read(result_path).unwrap()).unwrap();
    assert_eq!(result["passed"], true, "native accessibility provider did not expose guest window text");
}
