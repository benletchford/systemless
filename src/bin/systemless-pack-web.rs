use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let input = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: systemless-pack-web <input.sit> <output.kpk>".to_string())?;
    let output = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: systemless-pack-web <input.sit> <output.kpk>".to_string())?;
    if args.next().is_some() {
        return Err("usage: systemless-pack-web <input.sit> <output.kpk>".to_string());
    }

    let input_bytes =
        fs::read(&input).map_err(|e| format!("could not read {}: {e}", input.display()))?;
    let packed = systemless::game::pack_stuffit_for_web(&input_bytes)?;
    fs::write(&output, &packed)
        .map_err(|e| format!("could not write {}: {e}", output.display()))?;

    eprintln!(
        "packed {} -> {} ({} KB -> {} KB)",
        input.display(),
        output.display(),
        input_bytes.len() / 1024,
        packed.len() / 1024
    );
    Ok(())
}
