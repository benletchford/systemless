mod support;
use systemless_future::{
    events,
    load::LoadedApplication,
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn waiting_app(sleep: u16, region: u16) -> LoadedApplication {
    let f = support::pef_program(
        &[
            0x7fc802a6,
            0x38600008,
            0x38805000,
            0x38a00000 | u32::from(sleep),
            0x38c00000 | u32::from(region),
            0x81820008,
            0x800c0000,
            0x804c0004,
            0x7c0903a6,
            0x4e800421,
            0x7fc803a6,
            0x4e800020,
        ],
        &[b"WaitNextEvent"],
        12,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x5000), vec![0xa5; 16], Permissions::RW)
        .unwrap();
    app
}
#[test]
fn waiting_is_nonterminal_does_not_spin_and_time_wraps() {
    let mut app = waiting_app(5, 0);
    app.runtime
        .memory
        .write(GuestPtr(0x16a), &0xffff_fffeu32.to_be_bytes())
        .unwrap();
    assert_eq!(
        app.runtime.advance(100).unwrap(),
        Stop::Waiting { ticks: 5 }
    );
    let instructions = app.runtime.instructions;
    assert_eq!(
        app.runtime.advance(100).unwrap(),
        Stop::Waiting { ticks: 5 }
    );
    assert_eq!(app.runtime.instructions, instructions);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x5000), 16).unwrap(),
        &[0xa5; 16]
    );
    app.runtime.elapse_ticks(5).unwrap();
    assert_eq!(app.runtime.advance(0).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.instructions, instructions);
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Returned(0));
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x5006)).unwrap(), 3);
    assert!(app.runtime.elapse_ticks(1).is_err());
}
#[test]
fn only_matching_event_wakes_wait_and_zero_sleep_does_not_yield() {
    let mut app = waiting_app(10, 0);
    assert_eq!(
        app.runtime.advance(100).unwrap(),
        Stop::Waiting { ticks: 10 }
    );
    let input = app.runtime.process.input;
    events::post(&mut app.runtime.memory, 7, 11, 0, input).unwrap();
    assert_eq!(
        app.runtime.advance(100).unwrap(),
        Stop::Waiting { ticks: 10 }
    );
    events::post(&mut app.runtime.memory, 3, 42, 0, input).unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Returned(1));
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x5002)).unwrap(), 42);
    assert!(events::available(&app.runtime.memory, 128).unwrap());
    assert!(!events::available(&app.runtime.memory, 8).unwrap());
    assert_eq!(
        waiting_app(0, 0).runtime.advance(100).unwrap(),
        Stop::Returned(0)
    );
}
#[test]
fn unsupported_mouse_regions_fault_before_mutating_output() {
    let mut app = waiting_app(10, 1);
    assert!(app
        .runtime
        .advance(100)
        .unwrap_err()
        .to_string()
        .contains("mouse regions"));
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x5000), 16).unwrap(),
        &[0xa5; 16]
    );
    assert!(app.runtime.advance(100).is_err());
}

#[test]
fn headless_cli_resumes_after_wait_instead_of_reporting_yield_as_completion() {
    // Destination is writable PEF data at TOC+12; the program waits then returns.
    let f = support::pef_program(
        &[
            0x7fc802a6, 0x38600008, 0x3882000c, 0x38a00005, 0x38c00000, 0x81820008, 0x800c0000,
            0x804c0004, 0x7c0903a6, 0x4e800421, 0x7fc803a6, 0x4e800020,
        ],
        &[b"WaitNextEvent"],
        32,
    );
    let path = std::env::temp_dir().join(format!("systemless-wait-{}.pef", std::process::id()));
    std::fs::write(&path, f.bytes).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_systemless-future"))
        .arg("--pef")
        .arg(&path)
        .args(["--budget", "100"])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Returned(0)"), "{text}");
    assert!(!text.contains("Waiting"), "{text}");
}
