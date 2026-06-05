use std::path::PathBuf;
use std::time::Instant;

use systemless::cpu::Register;
use systemless::game;
use systemless::memory::MemoryBus;
use systemless::runner::{DEFAULT_REALTIME_CPU_MHZ, DEFAULT_VBL_HZ};

const DEFAULT_MAC_TIME_SECS: u32 = 3_786_912_000;
const DEFAULT_STEPS: usize = 20_000_000;
const DEFAULT_WARMUP_STEPS: usize = 500_000;

fn parse_usize_arg(args: &[String], index: usize, default: usize) -> usize {
    args.get(index)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn realtime_instructions_per_tick() -> u32 {
    let mhz = std::env::var("SYSTEMLESS_CPU_MHZ")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(DEFAULT_REALTIME_CPU_MHZ);
    (mhz * 1_000_000.0 / DEFAULT_VBL_HZ).round() as u32
}

fn env_flag_enabled(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().as_deref() {
        Some("1" | "true" | "True" | "TRUE" | "on" | "On" | "ON") => true,
        Some("0" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF") => false,
        _ => default,
    }
}

fn run_budget(runner: &mut systemless::runner::FixtureRunner, steps: usize) -> usize {
    let mut total = 0usize;
    while total < steps && !runner.is_halted() {
        let remaining = steps - total;
        let (ran, running) = runner.run_steps(remaining, None);
        total += ran;
        if !running || ran == 0 {
            break;
        }
    }
    total
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <game.sit|app> [steps] [warmup_steps]", args[0]);
        std::process::exit(2);
    }

    let game_path = PathBuf::from(&args[1]);
    let steps = parse_usize_arg(&args, 2, DEFAULT_STEPS);
    let warmup_steps = parse_usize_arg(&args, 3, DEFAULT_WARMUP_STEPS);

    let mut runner = game::new_runner();
    runner.set_app_start_time(DEFAULT_MAC_TIME_SECS);
    runner.set_instructions_per_tick(realtime_instructions_per_tick());
    runner.set_wait_sleep_cap_in_headless(Some(0));

    let app = game::load_game_from_path(&mut runner, &game_path).unwrap_or_else(|err| {
        eprintln!("load {}: {}", game_path.display(), err);
        std::process::exit(1);
    });
    game::init_game(&mut runner, &app);

    let warmup = run_budget(&mut runner, warmup_steps);
    if let Some(spec) = std::env::var("SYSTEMLESS_BENCH_DISASM").ok() {
        if let Some((pc_s, count_s)) = spec.split_once(':') {
            let pc_s = pc_s
                .trim()
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            if let (Ok(pc), Ok(count)) = (
                u32::from_str_radix(pc_s, 16),
                count_s.trim().parse::<usize>(),
            ) {
                for (addr, mnemonic, size) in runner.disassemble_at(pc, count) {
                    eprintln!(
                        "[BENCH-DISASM] ${:08X} {:<28} ; size {}",
                        addr, mnemonic, size
                    );
                }
            }
        }
    }
    if let Some(spec) = std::env::var("SYSTEMLESS_BENCH_BYTES").ok() {
        if let Some((addr_s, len_s)) = spec.split_once(':') {
            let addr_s = addr_s
                .trim()
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            if let (Ok(addr), Ok(len)) = (
                u32::from_str_radix(addr_s, 16),
                len_s.trim().parse::<usize>(),
            ) {
                for row in (0..len).step_by(16) {
                    let row_len = (len - row).min(16);
                    let bytes = (0..row_len)
                        .map(|i| {
                            format!(
                                "{:02X}",
                                runner.bus().read_byte(addr + row as u32 + i as u32)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    eprintln!("[BENCH-BYTES] ${:08X}: {}", addr + row as u32, bytes);
                }
            }
        }
    }
    let start_tick = runner.guest_tick();
    let start_traps = runner.dispatcher().trap_count;
    let start = Instant::now();
    let measured = run_budget(&mut runner, steps);
    let elapsed = start.elapsed().as_secs_f64();
    let mips = if elapsed > 0.0 {
        measured as f64 / elapsed / 1_000_000.0
    } else {
        0.0
    };
    let end_tick = runner.guest_tick();
    let traps = runner.dispatcher().trap_count.saturating_sub(start_traps);
    let pc = runner.cpu().read_reg(Register::PC);

    println!(
        "game={} warmup={} steps={} elapsed_s={:.6} mips={:.3} ticks={} traps={} pc=${:08X} halted={} batch_simple={} fast_byte_loops={} fast_ptinrect_scan={} inline_ptinrect={}",
        game_path.display(),
        warmup,
        measured,
        elapsed,
        mips,
        end_tick.wrapping_sub(start_tick),
        traps,
        pc,
        runner.is_halted(),
        env_flag_enabled("SYSTEMLESS_BATCH_SIMPLE", false),
        env_flag_enabled("SYSTEMLESS_FAST_BYTE_LOOPS", true),
        env_flag_enabled("SYSTEMLESS_FAST_PTINRECT_SCAN", true),
        env_flag_enabled("SYSTEMLESS_INLINE_PTINRECT", true),
    );
}
