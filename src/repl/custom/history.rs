// `:history` command — display the instruction history timeline.
//
// Reads `PreviewState::history` (a `Vec<HistoryEntry>`) and prints each entry
// with its Dockerfile line number, the raw instruction text, and its human-
// readable effect. This is a pure read command; it never mutates `PreviewState`.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::error::ReplError;

/// Execute the `:history` command.
///
/// Prints each `HistoryEntry` in insertion order, formatted as:
///
/// ```text
///   LINE  INSTRUCTION  ->  EFFECT
/// ```
///
/// Line numbers are right-aligned to the width of the largest line number in
/// the list, giving clean column alignment. When `state.history` is empty the
/// message `"No history recorded."` is printed instead.
///
/// All output goes to `writer`; I/O errors are mapped to
/// [`ReplError::InvalidArguments`].
pub fn execute(state: &PreviewState, writer: &mut impl Write) -> Result<(), ReplError> {
    // Map I/O errors into a ReplError so callers have a uniform error type.
    let io_err = |e: std::io::Error| ReplError::InvalidArguments {
        command: ":history".to_string(),
        message: e.to_string(),
    };

    if state.history.is_empty() {
        return writeln!(writer, "No history recorded.").map_err(io_err);
    }

    // Compute the width needed to right-align the largest line number.
    // This keeps the instruction column at a consistent horizontal position
    // regardless of how many digits the line numbers require.
    let max_line = state.history.iter().map(|e| e.line).max().unwrap_or(1); // unwrap is safe: we checked `is_empty()` above
    let line_width = max_line.to_string().len();

    for entry in &state.history {
        // Sanitize user-controlled fields before writing to the terminal.
        // `entry.instruction` contains raw Dockerfile instruction text and can
        // embed ANSI escape sequences that would be interpreted by the terminal
        // emulator. `entry.effect` is engine-generated but sanitized
        // defensively since it may include user-supplied tokens in the future.
        let safe_instruction = sanitize_for_terminal(&entry.instruction);
        let safe_effect = sanitize_for_terminal(&entry.effect);

        writeln!(
            writer,
            "{:>width$}  {}  ->  {}",
            entry.line,
            safe_instruction,
            safe_effect,
            width = line_width,
        )
        .map_err(io_err)?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::state::{HistoryEntry, PreviewState};

    /// Run `execute` and return the captured output as a `String`.
    fn run(state: &PreviewState) -> String {
        let mut buf = Vec::new();
        execute(state, &mut buf).expect(":history should not fail");
        String::from_utf8(buf).expect("output must be utf-8")
    }

    // --- Empty state ---

    #[test]
    fn empty_history_prints_no_history_message() {
        let state = PreviewState::default();
        let out = run(&state);
        assert_eq!(out.trim(), "No history recorded.");
    }

    // --- Single entry ---

    #[test]
    fn single_entry_shows_line_instruction_and_effect() {
        let mut state = PreviewState::default();
        state.history.push(HistoryEntry {
            line: 3,
            instruction: "RUN apt-get install -y curl".to_string(),
            effect: "recorded 1 apt package".to_string(),
        });
        let out = run(&state);
        // Line number, instruction text, and effect must all appear.
        assert!(out.contains('3'), "line number must appear; got: {out}");
        assert!(
            out.contains("RUN apt-get install -y curl"),
            "instruction must appear; got: {out}"
        );
        assert!(
            out.contains("recorded 1 apt package"),
            "effect must appear; got: {out}"
        );
        // Arrow separator must also appear.
        assert!(
            out.contains("->"),
            "arrow separator must appear; got: {out}"
        );
    }

    // --- Multiple entries appear in insertion order ---

    #[test]
    fn multiple_entries_appear_in_insertion_order() {
        let mut state = PreviewState::default();
        state.history.push(HistoryEntry {
            line: 3,
            instruction: "RUN apt-get install -y curl".to_string(),
            effect: "recorded 1 apt package".to_string(),
        });
        state.history.push(HistoryEntry {
            line: 7,
            instruction: "RUN pip install flask".to_string(),
            effect: "recorded 1 pip package".to_string(),
        });
        let out = run(&state);

        // Both entries must appear in the correct order.
        let pos_curl = out
            .find("apt-get install -y curl")
            .expect("curl entry must appear");
        let pos_flask = out
            .find("pip install flask")
            .expect("flask entry must appear");
        assert!(
            pos_curl < pos_flask,
            "curl entry must precede flask entry; got: {out}"
        );
    }

    // --- Line number alignment with wide numbers ---

    #[test]
    fn line_numbers_are_right_aligned_by_max_width() {
        let mut state = PreviewState::default();
        // Line 3 has a single-digit number; line 100 has three digits.
        // Both should be right-aligned to width 3.
        state.history.push(HistoryEntry {
            line: 3,
            instruction: "FROM ubuntu:22.04".to_string(),
            effect: "set base image".to_string(),
        });
        state.history.push(HistoryEntry {
            line: 100,
            instruction: "RUN echo done".to_string(),
            effect: "simulated shell command".to_string(),
        });
        let out = run(&state);

        // The output for line 3 should be right-padded to width 3 (i.e. "  3").
        assert!(
            out.contains("  3  FROM"),
            "line 3 must be right-aligned to width 3; got: {out}"
        );
        // The output for line 100 should appear without extra leading spaces.
        assert!(
            out.contains("100  RUN"),
            "line 100 must appear without extra leading spaces; got: {out}"
        );
    }

    // --- Sanitization of user-controlled fields ---

    #[test]
    fn escape_sequence_in_instruction_is_stripped_from_output() {
        let mut state = PreviewState::default();
        // Embed an ANSI escape sequence in the raw instruction text — this
        // mirrors a Dockerfile author placing control bytes in a RUN argument.
        state.history.push(HistoryEntry {
            line: 1,
            instruction: "RUN echo \x1b[2J".to_string(),
            effect: "simulated shell command".to_string(),
        });
        let buf = {
            let mut b = Vec::new();
            execute(&state, &mut b).expect("should succeed");
            b
        };
        // The ESC byte (0x1B) must not appear in the printed output.
        assert!(
            !buf.contains(&0x1B),
            "ESC must be stripped from instruction output"
        );
        // The non-escape text should still appear.
        let out = String::from_utf8(buf).expect("utf-8");
        assert!(out.contains("RUN echo"), "got: {out}");
    }

    #[test]
    fn escape_sequence_in_effect_is_stripped_from_output() {
        let mut state = PreviewState::default();
        // Also verify that the effect field (engine-generated but sanitized
        // defensively) does not pass control bytes through.
        state.history.push(HistoryEntry {
            line: 2,
            instruction: "ENV PATH=/usr/bin".to_string(),
            effect: "set PATH\x1b[0m".to_string(),
        });
        let buf = {
            let mut b = Vec::new();
            execute(&state, &mut b).expect("should succeed");
            b
        };
        assert!(
            !buf.contains(&0x1B),
            "ESC must be stripped from effect output"
        );
    }
}
