//! CLI argument definitions for `demu`.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "demu",
    version,
    about = "Fast, non-destructive Docker/Compose preview shell"
)]
pub struct Cli {
    /// Dockerfile or Compose file to preview
    #[arg(short = 'f', long = "file")]
    pub file: std::path::PathBuf,

    /// Target stage (multi-stage Dockerfile builds)
    #[arg(long)]
    pub stage: Option<String>,

    /// Switch to Compose mode (use with -f compose.yaml)
    #[arg(long)]
    pub compose: bool,

    /// Service to inspect — required when --compose is set
    #[arg(long)]
    pub service: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_flags_are_parsed() {
        let cli = Cli::try_parse_from([
            "demu",
            "--compose",
            "-f",
            "compose.yaml",
            "--service",
            "api",
        ])
        .expect("should parse");
        assert!(cli.compose);
        assert_eq!(cli.service, Some("api".to_string()));
        assert_eq!(cli.file.to_str().unwrap(), "compose.yaml");
    }

    #[test]
    fn compose_flag_defaults_to_false() {
        let cli = Cli::try_parse_from(["demu", "-f", "Dockerfile"]).expect("should parse");
        assert!(!cli.compose);
        assert!(cli.service.is_none());
    }

    #[test]
    fn service_without_compose_parses() {
        // Validation is in main.rs, not clap — so this must parse without error.
        let cli = Cli::try_parse_from(["demu", "-f", "compose.yaml", "--service", "api"])
            .expect("should parse even without --compose");
        assert!(!cli.compose);
        assert_eq!(cli.service, Some("api".to_string()));
    }
}
