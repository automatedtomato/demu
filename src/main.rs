use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "demu",
    version,
    about = "Fast, non-destructive Docker/Compose preview shell"
)]
struct Cli {
    /// Dockerfile to preview
    #[arg(short = 'f', long = "file")]
    file: std::path::PathBuf,

    /// Target stage (multi-stage builds)
    #[arg(long)]
    stage: Option<String>,
}

fn main() -> Result<()> {
    let _cli = Cli::parse();
    println!("demu: preview not yet implemented");
    Ok(())
}
