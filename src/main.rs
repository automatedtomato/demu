// `demu` binary entrypoint.
//
// Wires together the full preview pipeline:
//   1. Parse CLI arguments with clap (`Cli`).
//   2. Validate the Dockerfile path (exists and is a regular file).
//   3. Canonicalize the path to derive an absolute build-context directory.
//   4. Read the file contents.
//   5. Parse with `parser::dockerfile::parse_dockerfile`.
//   6. Run the engine with `engine::run`.
//   7. Print any engine warnings to stderr.
//   8. Hand the resulting `PreviewState` to the interactive REPL.
//
// `run_cli` returns `anyhow::Result<()>` so that every error path funnels
// through the single `main` handler, which formats the message as
// `"demu: <err>"` before exiting with code 1.

use anyhow::{Context, Result};
use clap::Parser;
use demu::{
    engine, output::sanitize::sanitize_for_terminal, parser::dockerfile::parse_dockerfile,
    repl::config::ReplConfig, repl::run_repl, Cli,
};

/// Full CLI pipeline. Returns `Err` for any unrecoverable failure.
///
/// Keeping this out of `main` keeps the logic testable and makes the error
/// formatting in `main` trivially straightforward.
fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    // ── 1. Validate the path ─────────────────────────────────────────────────

    // The path must exist — surface a clear error rather than a confusing I/O
    // message later.
    if !cli.file.exists() {
        anyhow::bail!("Dockerfile not found: '{}'", cli.file.display());
    }

    // The path must be a regular file, not a directory or device node.
    if !cli.file.is_file() {
        anyhow::bail!(
            "'{}' is not a regular file — pass the path to a Dockerfile",
            cli.file.display()
        );
    }

    // ── 2. Derive the build context directory ────────────────────────────────

    // Canonicalize gives us the absolute path, resolving symlinks. We use the
    // parent directory as the build context, consistent with `docker build`.
    let canonical = cli
        .file
        .canonicalize()
        .with_context(|| format!("cannot resolve path '{}'", cli.file.display()))?;

    let context_dir = canonical
        .parent()
        .with_context(|| {
            format!(
                "cannot determine parent directory of '{}'",
                canonical.display()
            )
        })?
        .to_path_buf();

    // ── 3. Read the Dockerfile ───────────────────────────────────────────────

    let content = std::fs::read_to_string(&canonical)
        .with_context(|| format!("cannot read '{}'", canonical.display()))?;

    // ── 4. Parse ─────────────────────────────────────────────────────────────

    let instructions = parse_dockerfile(&content)
        .with_context(|| format!("failed to parse '{}'", canonical.display()))?;

    // ── 5. Run the engine ────────────────────────────────────────────────────

    let output = engine::run(instructions, &context_dir)
        .with_context(|| format!("engine error while processing '{}'", canonical.display()))?;

    // ── 5a. Select the target stage ──────────────────────────────────────────

    // If `--stage` is provided, look it up in the registry (by name or numeric
    // index). If omitted, use the final stage returned directly by the engine.
    let mut state = if let Some(ref stage_name) = cli.stage {
        output.stages.get(stage_name).cloned().ok_or_else(|| {
            // Sanitize both the user-supplied stage name and the Dockerfile-derived
            // alias strings before embedding them in the error message. The main
            // handler applies sanitize_for_terminal as a final backstop, but
            // sanitizing at construction keeps the error value clean regardless of
            // where it is later consumed or logged.
            let safe_name = sanitize_for_terminal(stage_name);
            let available = output
                .stages
                .keys()
                .into_iter()
                .map(|k| sanitize_for_terminal(&k))
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::anyhow!(
                "stage '{}' not found; available stages: {}",
                safe_name,
                available
            )
        })?
    } else {
        output.state
    };

    // ── 6. Print warnings to stderr ──────────────────────────────────────────

    // Warnings are non-fatal diagnostics that tell the user where the
    // simulation is approximate or incomplete. Sanitize before printing —
    // Warning strings embed user-supplied data from the Dockerfile (image
    // names, instruction text, paths) that could contain ANSI escape bytes.
    for warning in &state.warnings {
        let safe = sanitize_for_terminal(&warning.to_string());
        eprintln!("warning: {safe}");
    }

    // ── 7. Enter the interactive REPL ────────────────────────────────────────

    // Build the session-level config that the REPL needs for `:reload`.
    // The canonical path and context_dir are already computed above.
    let repl_config = ReplConfig::new(canonical).with_selected_stage(cli.stage.clone());

    run_repl(&mut state, &repl_config)?;

    Ok(())
}

fn main() {
    // All errors funnel here. Prefixing with "demu:" mirrors standard Unix tool
    // conventions (e.g. `git: …`, `cargo: …`) and makes the source obvious when
    // output is piped or logged.
    if let Err(err) = run_cli() {
        // Sanitize the error string — anyhow error chains include user-supplied
        // data (file paths, instruction text) that could embed ANSI escape bytes.
        let safe = sanitize_for_terminal(&err.to_string());
        eprintln!("demu: {safe}");
        std::process::exit(1);
    }
}
