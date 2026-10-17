//! Opt-in, read-only register and bounded live-allocation snapshots.
use systemless_future::{cpu::Cpu, load::LoadedApplication, memory::GuestPtr};
pub fn report(app: &LoadedApplication) {
    println!(
        "cpu diagnostic completed callbacks: sound={} timer={} vbl={} audio_work_pending={}",
        app.runtime.services.completed_sound_callbacks,
        app.runtime.services.completed_timer_callbacks,
        app.runtime.services.completed_vbl_callbacks,
        app.runtime.process.sound.playing(),
    );
    match &app.runtime.cpu {
        Cpu::M68k(c) => {
            println!(
                "cpu diagnostic 68k pc={:08x} registers={:08x?}",
                c.pc, c.dar
            );
            for (index, &address) in c.dar[8..].iter().enumerate() {
                candidate(app, &format!("A{index}"), address);
            }
        }
        Cpu::PowerPc(c) => {
            println!(
                "cpu diagnostic ppc pc={:08x} registers={:08x?}",
                c.pc, c.gpr
            );
            for (index, &address) in c.gpr.iter().enumerate() {
                candidate(app, &format!("R{index}"), address);
            }
        }
    }
}
fn candidate(app: &LoadedApplication, label: &str, address: u32) {
    if let Some((base, length)) = app.runtime.process.allocation_containing(GuestPtr(address)) {
        let remaining = length - (address - base.0);
        let count = remaining.min(256) as usize;
        println!("cpu diagnostic {label}={address:08x} physical allocation={:08x}+{length} bytes={:02x?}", base.0, app.runtime.memory.read(GuestPtr(address), count));
    }
}
