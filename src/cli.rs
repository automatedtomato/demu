//! CLI argument definitions for `demu`.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "demu",
    version,
    about = "Fast, non-destructive Docker/Compose preview shell"
)]
pub struct Cli {
    /// Dockerfile to preview
    #[arg(short = 'f', long = "file")]
    pub file: std::path::PathBuf,

    /// Target stage (multi-stage builds)
    #[arg(long)]
    pub stage: Option<String>,
}
