use anyhow::Result;
use clap::Parser;
use demu::Cli;

fn main() -> Result<()> {
    let _cli = Cli::parse();
    println!("demu: preview not yet implemented");
    Ok(())
}
