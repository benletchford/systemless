use anyhow::{ensure, Result};
use clap::{Args, Parser, Subcommand};
use std::{collections::BTreeMap, fs, path::PathBuf};
use systemless_catalogue_tools::catalogue_tools::{
    assets,
    catalogue::{self, Mode},
    r2,
};

#[derive(Parser)]
#[command(
    version,
    about = "Validate, compile and maintain the Systemless Community Catalogue"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}
#[derive(Args, Clone, Default)]
struct CheckMode {
    /// Reject all incoming files (required on the default branch).
    #[arg(long)]
    no_incoming: bool,
    /// Also require every hosted asset to have been promoted to SHA-256 storage.
    #[arg(long)]
    production: bool,
}
impl CheckMode {
    fn mode(&self) -> Mode {
        if self.production {
            Mode::Production
        } else if self.no_incoming {
            Mode::NoIncoming
        } else {
            Mode::Preview
        }
    }
}
#[derive(Subcommand)]
enum Command {
    Check {
        #[command(flatten)]
        mode: CheckMode,
    },
    Build {
        #[command(flatten)]
        mode: CheckMode,
        #[arg(long, default_value = "dist/games.rs")]
        output: PathBuf,
    },
    Stats,
    /// Generate static HTML pages from the checked-in Markdown.
    Pages {
        #[arg(long)]
        template: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "https://systemless.org")]
        origin: String,
    },
    /// List all asset references, or promote staged files.
    Assets {
        #[command(subcommand)]
        command: Option<AssetCommand>,
    },
    /// Alias for the same generic asset promotion pipeline.
    Media {
        #[command(subcommand)]
        command: AssetCommand,
    },
    R2 {
        #[command(subcommand)]
        command: R2Command,
    },
}
#[derive(Subcommand)]
enum AssetCommand {
    /// Download and validate artifacts locally without uploading or changing entries.
    Fetch {
        #[arg(long)]
        entry: Option<String>,
        /// Local cache directory; defaults to <root>/catalogue/.downloads.
        #[arg(long)]
        directory: Option<PathBuf>,
    },
    /// Complete an interrupted promotion using its local transaction journal.
    Recover,
    /// Validate/hash all pending assets; uploads and rewrites require --apply.
    Promote {
        #[arg(long)]
        entry: Option<String>,
        #[arg(long)]
        apply: bool,
        /// Dedicated local fixture backend for offline promotion rehearsals.
        #[arg(long)]
        local_store: Option<PathBuf>,
    },
}
#[derive(Args)]
struct PolicyArgs {
    #[arg(long, default_value_t = 10)]
    max_delete: usize,
    #[arg(long, default_value_t = 10)]
    max_delete_percent: u8,
    #[arg(long, default_value_t = 168)]
    min_age_hours: u32,
    #[arg(long)]
    allow_empty: bool,
}
impl From<PolicyArgs> for r2::DeletePolicy {
    fn from(p: PolicyArgs) -> Self {
        Self {
            max_delete: p.max_delete,
            max_delete_percent: p.max_delete_percent,
            min_age_hours: p.min_age_hours,
            allow_empty: p.allow_empty,
        }
    }
}
#[derive(Subcommand)]
enum R2Command {
    /// Fetch a complete paginated inventory of both managed prefixes.
    Inventory {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Compute a reviewable plan; --inventory makes this command credential-free.
    Plan {
        #[arg(long)]
        inventory: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[command(flatten)]
        policy: PolicyArgs,
    },
    /// Revalidate a reviewed plan against live R2 before applying bounded deletions.
    Reconcile {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        apply: bool,
    },
}
fn output(value: &impl serde::Serialize, path: Option<&std::path::Path>) -> Result<()> {
    let bytes = catalogue::json_bytes(value)?;
    if let Some(path) = path {
        catalogue::atomic_write(path, &bytes)?;
    } else {
        use std::io::Write;
        std::io::stdout().lock().write_all(&bytes)?;
    }
    Ok(())
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { mode } => {
            let c = catalogue::load(&cli.root, mode.mode())?;
            eprintln!("Validated {} entries", c.documents.len());
        }
        Command::Build { mode, output: path } => {
            let c = catalogue::load(&cli.root, mode.mode())?;
            catalogue::atomic_write(
                &path,
                systemless_catalogue_tools::catalogue_tools::site::rust_games(&catalogue::build(
                    &c,
                )?)?
                .as_bytes(),
            )?;
            eprintln!(
                "Built {} entries into {}",
                c.documents.len(),
                path.display()
            );
        }
        Command::Stats => {
            let c = catalogue::load(&cli.root, Mode::Preview)?;
            let mut kinds = BTreeMap::<_, usize>::new();
            let mut statuses = BTreeMap::<_, usize>::new();
            let mut architectures = BTreeMap::<_, usize>::new();
            let mut launch_enabled = 0;
            let mut pending = 0;
            let mut external = 0;
            for d in &c.documents {
                *kinds.entry(d.entry.kind).or_default() += 1;
                *statuses.entry(d.entry.compatibility.status).or_default() += 1;
                for a in &d.entry.architectures {
                    *architectures.entry(*a).or_default() += 1;
                }
                launch_enabled += usize::from(d.entry.launch_enabled);
                for a in &d.entry.artifacts {
                    match a.source {
                        systemless_catalogue_tools::catalogue_tools::AssetSource::Incoming {
                            ..
                        }
                        | systemless_catalogue_tools::catalogue_tools::AssetSource::Url {
                            ..
                        } => pending += 1,
                        systemless_catalogue_tools::catalogue_tools::AssetSource::External {
                            ..
                        } => external += 1,
                        _ => {}
                    }
                }
            }
            let objects = assets::desired(&c)?;
            output(
                &serde_json::json!({"entries":c.documents.len(),"launch_enabled":launch_enabled,"kinds":kinds,"compatibility":statuses,"architectures":architectures,"unique_objects":objects.len(),"unique_bytes":objects.iter().map(|o| o.size_bytes as u128).sum::<u128>(),"pending_assets":pending,"external_links":external}),
                None,
            )?;
        }
        Command::Pages {
            template,
            output,
            origin,
        } => {
            let c = catalogue::load(&cli.root, Mode::Production)?;
            systemless_catalogue_tools::catalogue_tools::site::write_pages(
                &catalogue::build(&c)?,
                &template,
                &output,
                &origin,
            )?;
        }
        Command::Assets { command: None } => {
            let c = catalogue::load(&cli.root, Mode::Preview)?;
            let refs:Vec<_> = c.documents.iter().flat_map(|d| d.entry.artifacts.iter().map(|a| serde_json::json!({"entry":d.entry.id,"artifact":a.id,"source":a.source,"url":catalogue::artifact_url(&c.config,a),"format":a.format,"role":a.role}))).collect();
            output(
                &serde_json::json!({"objects":assets::desired(&c)?,"references":refs}),
                None,
            )?;
        }
        Command::Assets {
            command: Some(command),
        }
        | Command::Media { command } => match command {
            AssetCommand::Fetch { entry, directory } => {
                let directory = directory.unwrap_or_else(|| cli.root.join("catalogue/.downloads"));
                output(
                    &assets::fetch(&cli.root, entry.as_deref(), &directory)?,
                    None,
                )?;
            }
            AssetCommand::Recover => output(&assets::recover(&cli.root)?, None)?,
            AssetCommand::Promote {
                entry,
                apply,
                local_store,
            } => {
                let mut store: Box<dyn assets::ObjectStore> = if !apply {
                    Box::new(assets::NoUpload)
                } else if let Some(root) = local_store {
                    Box::new(assets::DirectoryStore { root })
                } else {
                    Box::new(r2::R2Store::from_env()?)
                };
                output(
                    &assets::promote(&cli.root, entry.as_deref(), apply, store.as_mut())?,
                    None,
                )?;
            }
        },
        Command::R2 { command } => match command {
            R2Command::Inventory { output: path } => {
                output(&r2::R2Store::from_env()?.inventory()?, path.as_deref())?
            }
            R2Command::Plan {
                inventory,
                output: path,
                policy,
            } => {
                let c = catalogue::load(&cli.root, Mode::Preview)?;
                let inventory = if let Some(path) = inventory {
                    serde_json::from_slice(&fs::read(path)?)?
                } else {
                    r2::R2Store::from_env()?.inventory()?
                };
                let plan = r2::plan(&c, &inventory, policy.into(), jiff::Timestamp::now())?;
                output(&plan, path.as_deref())?;
                ensure!(
                    plan.blockers.is_empty(),
                    "plan is blocked; inspect its blockers before proceeding"
                );
            }
            R2Command::Reconcile { plan, apply } => {
                let plan: r2::Plan = serde_json::from_slice(&fs::read(plan)?)?;
                ensure!(
                    apply,
                    "reconciliation requires a reviewed --plan and explicit --apply"
                );
                let count = r2::reconcile(&cli.root, &plan, &r2::R2Store::from_env()?)?;
                eprintln!("Deleted {count} orphaned catalogue objects");
            }
        },
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
