use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cmd::{attach, run, snapshot};

#[derive(Parser, Debug)]
#[command(
    name = "allocmap",
    version,
    about = "Real-time heap memory profiler — attach to running processes without restart",
    long_about = None,
    after_help = "For more information visit: https://github.com/handsomevictor/allocmap"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Attach to a running process and show live memory usage in a TUI
    Attach(attach::AttachArgs),

    /// Start a new process with LD_PRELOAD instrumentation (more complete data)
    Run(run::RunArgs),

    /// Take a non-interactive snapshot and output JSON (suitable for CI/CD)
    Snapshot(snapshot::SnapshotArgs),
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Commands::Attach(args) => attach::execute(args).await,
            Commands::Run(args) => run::execute(args).await,
            Commands::Snapshot(args) => snapshot::execute(args).await,
        }
    }
}
