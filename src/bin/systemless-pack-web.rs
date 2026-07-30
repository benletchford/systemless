use std::fs;
use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "systemless-pack-web",
    version,
    about = "Pack StuffIt archives and HFS disk images for the Systemless web player"
)]
struct Cli {
    /// Primary StuffIt archive or HFS disk image to pack
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// Destination for the packed archive
    #[arg(value_name = "OUTPUT")]
    output: PathBuf,

    /// Additional StuffIt archive or HFS disk image to merge
    #[arg(long = "additional-input", value_name = "INPUT")]
    additional_inputs: Vec<PathBuf>,

    /// Retain files at or below this normalized classic Mac path
    #[arg(long = "include-prefix", value_name = "PATH")]
    include_prefixes: Vec<String>,
}

fn main() -> Result<(), String> {
    let cli = Cli::parse();

    let mut input_paths = vec![cli.input.clone()];
    input_paths.extend(cli.additional_inputs);
    let input_bytes = input_paths
        .iter()
        .map(|path| fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display())))
        .collect::<Result<Vec<_>, _>>()?;
    let source_refs = input_bytes.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let include_prefixes = cli
        .include_prefixes
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let packed = systemless::game::pack_game_sources_for_web(&source_refs, &include_prefixes)?;
    fs::write(&cli.output, &packed)
        .map_err(|e| format!("could not write {}: {e}", cli.output.display()))?;

    eprintln!(
        "packed {} source(s) -> {} ({} KB -> {} KB)",
        input_paths.len(),
        cli.output.display(),
        input_bytes.iter().map(Vec::len).sum::<usize>() / 1024,
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
        assert!(cli.additional_inputs.is_empty());
        assert!(cli.include_prefixes.is_empty());
    }

    #[test]
    fn cli_parses_additional_sources_and_include_prefixes() {
        let cli = Cli::try_parse_from([
            "systemless-pack-web",
            "application.sit",
            "game.kpk",
            "--additional-input",
            "cd.img",
            "--include-prefix",
            "Game",
            "--include-prefix",
            "Game CD/Data",
        ])
        .expect("multi-source options should parse");

        assert_eq!(cli.additional_inputs, vec![PathBuf::from("cd.img")]);
        assert_eq!(cli.include_prefixes, vec!["Game", "Game CD/Data"]);
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
