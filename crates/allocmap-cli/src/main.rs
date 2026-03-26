mod cli;
mod cmd;
mod error;

use anyhow::Result;
use cli::Cli;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.run().await
}
