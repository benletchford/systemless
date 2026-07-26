use std::fs;
use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "systemless-pack-web",
    version,
    about = "Pack a StuffIt archive for the Systemless web player"
)]
struct Cli {
    /// StuffIt archive to pack
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// Destination for the packed archive
    #[arg(value_name = "OUTPUT")]
    output: PathBuf,
}

fn main() -> Result<(), String> {
    let cli = Cli::parse();

    let input_bytes =
        fs::read(&cli.input).map_err(|e| format!("could not read {}: {e}", cli.input.display()))?;
    let packed = systemless::game::pack_stuffit_for_web(&input_bytes)?;
    fs::write(&cli.output, &packed)
        .map_err(|e| format!("could not write {}: {e}", cli.output.display()))?;

    eprintln!(
        "packed {} -> {} ({} KB -> {} KB)",
        cli.input.display(),
        cli.output.display(),
        input_bytes.len() / 1024,
        packed.len() / 1024
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn cli_parses_input_and_output_paths() {
        let cli = Cli::try_parse_from(["systemless-pack-web", "input.sit", "output.kpk"])
            .expect("paths should parse");

        assert_eq!(cli.input, PathBuf::from("input.sit"));
        assert_eq!(cli.output, PathBuf::from("output.kpk"));
    }

    #[test]
    fn cli_generates_help_and_version() {
        let help = Cli::try_parse_from(["systemless-pack-web", "--help"])
            .expect_err("--help should stop parsing");
        let version = Cli::try_parse_from(["systemless-pack-web", "--version"])
            .expect_err("--version should stop parsing");

        assert_eq!(help.kind(), ErrorKind::DisplayHelp);
        assert_eq!(version.kind(), ErrorKind::DisplayVersion);
    }

    #[test]
    fn cli_requires_both_paths_and_rejects_extra_arguments() {
        let missing_output = Cli::try_parse_from(["systemless-pack-web", "input.sit"])
            .expect_err("output path should be required");
        let extra_argument =
            Cli::try_parse_from(["systemless-pack-web", "input.sit", "output.kpk", "extra"])
                .expect_err("extra arguments should be rejected");

        assert_eq!(missing_output.kind(), ErrorKind::MissingRequiredArgument);
        assert_eq!(extra_argument.kind(), ErrorKind::UnknownArgument);
    }
}
