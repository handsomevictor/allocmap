use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cmd::{attach, run, snapshot};

#[derive(Parser, Debug)]
#[command(
    name = "allocmap",
    version,
    about = "实时内存分析工具 — 无需重启进程，直接 attach 观察内存行为",
    long_about = None,
    after_help = "更多信息请访问：https://github.com/handsomevictor/allocmap"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Attach 到正在运行的进程，实时显示内存使用情况
    Attach(attach::AttachArgs),

    /// 以 LD_PRELOAD 模式启动新进程（数据更完整）
    Run(run::RunArgs),

    /// 非交互式快照，输出 JSON（适合 CI/CD）
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
