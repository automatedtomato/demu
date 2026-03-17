//! Line-based Dockerfile parser for the v0.1 instruction subset.
//!
//! Supported instructions: FROM, WORKDIR, COPY, ENV, RUN.
//! All other instructions are captured as `Instruction::Unknown` so the
//! engine can record them in history and emit a warning without panicking.
//!
//! This parser deliberately keeps things simple: one instruction per line,
//! no continuation backslash support, no here-doc RUN syntax. These
//! approximations are acceptable for the preview-shell use case.

use std::path::PathBuf;

use crate::model::instruction::{CopySource, Instruction};
use crate::parser::error::ParseError;

/// Parse a Dockerfile string into a sequence of typed instructions.
///
/// Lines that are empty or start with `#` are silently skipped.
/// Instructions that are not in the v0.1 subset are returned as
/// `Instruction::Unknown` rather than causing an error.
///
/// # Errors
///
/// Returns `ParseError::InvalidInstruction` when a known instruction is
/// present but its arguments are malformed (e.g. `FROM` with no image,
/// `COPY` with fewer than two path arguments).
pub fn parse_dockerfile(input: &str) -> Result<Vec<Instruction>, ParseError> {
    let mut instructions = Vec::new();

    for (idx, line) in input.lines().enumerate() {
        // Line numbers are 1-based for user-visible error messages.
        let line_num = idx + 1;
        let trimmed = line.trim();

        // Skip blank lines and comment lines.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Split the line into the keyword and everything that follows it.
        // If there is no whitespace, `rest` is an empty string.
        let (keyword, rest) = split_keyword(trimmed);

        // Dispatch on the keyword, case-insensitively.
        let instruction = match keyword.to_ascii_lowercase().as_str() {
            "from" => parse_from(rest, line_num)?,
            "workdir" => parse_workdir(rest, line_num)?,
            "copy" => {
                // COPY with a flag (e.g. --from=builder) is unsupported in v0.1.
                // Downgrade to Unknown so the line is preserved in history.
                if rest.trim_start().starts_with("--") {
                    Instruction::Unknown {
                        raw: trimmed.to_string(),
                    }
                } else {
                    // Multi-source COPY (`COPY src1 src2 dest`) is also not
                    // supported in v0.1. Silently truncating to the first two
                    // tokens would store the wrong dest, which is worse than
                    // surfacing the limitation. Downgrade to Unknown instead.
                    let token_count = rest.split_whitespace().count();
                    if token_count > 2 {
                        Instruction::Unknown {
                            raw: trimmed.to_string(),
                        }
                    } else {
                        parse_copy(rest, line_num)?
                    }
                }
            }
            "env" => parse_env(rest, line_num)?,
            "run" => parse_run(rest, line_num)?,
            // Any other keyword is preserved as-is for history and warnings.
            _ => Instruction::Unknown {
                raw: trimmed.to_string(),
            },
        };

        instructions.push(instruction);
    }

    Ok(instructions)
}

/// Split `line` on the first run of whitespace.
///
/// Returns `(keyword, rest)` where `rest` is everything after the first
/// whitespace token. If the line has no whitespace, `rest` is `""`.
fn split_keyword(line: &str) -> (&str, &str) {
    match line.find(|c: char| c.is_ascii_whitespace()) {
        Some(pos) => {
            let keyword = &line[..pos];
            let rest = line[pos..].trim_start();
            (keyword, rest)
        }
        None => (line, ""),
    }
}

/// Parse a `FROM <image> [AS <alias>]` instruction.
///
/// Returns an error if the image token is absent or if `AS` appears
/// without a following alias name.
fn parse_from(rest: &str, line: usize) -> Result<Instruction, ParseError> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();

    if tokens.is_empty() {
        return Err(ParseError::InvalidInstruction {
            line,
            message: "FROM requires an image name".to_string(),
        });
    }

    let image = tokens[0].to_string();

    // Check for optional `AS <alias>` clause.
    let alias = if tokens.len() >= 2 && tokens[1].eq_ignore_ascii_case("as") {
        // `AS` was present — the alias token must follow it.
        match tokens.get(2) {
            Some(a) => Some((*a).to_string()),
            None => {
                return Err(ParseError::InvalidInstruction {
                    line,
                    message: "FROM … AS requires a stage alias after AS".to_string(),
                });
            }
        }
    } else {
        None
    };

    // Any tokens beyond `image [AS alias]` are unexpected.
    // `FROM image` → max 1 token. `FROM image AS alias` → max 3 tokens.
    let expected_max = if alias.is_some() { 3 } else { 1 };
    if tokens.len() > expected_max {
        return Err(ParseError::InvalidInstruction {
            line,
            message: format!(
                "FROM has unexpected extra tokens: {}",
                tokens[expected_max..].join(" ")
            ),
        });
    }

    Ok(Instruction::From { image, alias })
}

/// Parse a `WORKDIR <path>` instruction.
///
/// Returns an error if the path argument is missing.
fn parse_workdir(rest: &str, line: usize) -> Result<Instruction, ParseError> {
    let path = rest.trim();

    if path.is_empty() {
        return Err(ParseError::InvalidInstruction {
            line,
            message: "WORKDIR requires a path argument".to_string(),
        });
    }

    Ok(Instruction::Workdir {
        path: PathBuf::from(path),
    })
}

/// Parse a `COPY <src> <dest>` instruction.
///
/// The `--from` flag check happens in the caller before this function
/// is invoked, so `rest` is guaranteed not to start with `--` here.
///
/// Returns an error if fewer than two path tokens are present.
fn parse_copy(rest: &str, line: usize) -> Result<Instruction, ParseError> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();

    if tokens.len() < 2 {
        return Err(ParseError::InvalidInstruction {
            line,
            message: "COPY requires at least two arguments: <src> <dest>".to_string(),
        });
    }

    let source = CopySource::Host(PathBuf::from(tokens[0]));
    let dest = PathBuf::from(tokens[1]);

    Ok(Instruction::Copy { source, dest })
}

/// Parse an `ENV <key>=<value>` or `ENV <key> <value>` instruction.
///
/// Two syntactic forms are supported:
/// - `ENV KEY=VALUE` — split on the first `=`.
/// - `ENV KEY VALUE` — split on the first whitespace; value may be empty.
///
/// Returns an error if the key is empty.
fn parse_env(rest: &str, line: usize) -> Result<Instruction, ParseError> {
    let (key, value) = if let Some(eq_pos) = rest.find('=') {
        // `KEY=VALUE` form — everything before `=` is the key.
        let key = rest[..eq_pos].trim().to_string();
        let value = rest[eq_pos + 1..].to_string();
        (key, value)
    } else {
        // `KEY VALUE` form — split on first whitespace. The value is
        // everything after the first whitespace run; whitespace is NOT
        // trimmed from the value side so `ENV PATH  /usr/bin ` stores
        // the value exactly as written (consistent with the `=` form).
        let mut parts = rest.splitn(2, |c: char| c.is_ascii_whitespace());
        let key = parts.next().unwrap_or("").trim().to_string();
        let value = parts.next().unwrap_or("").to_string();
        (key, value)
    };

    if key.is_empty() {
        return Err(ParseError::InvalidInstruction {
            line,
            message: "ENV requires a non-empty key".to_string(),
        });
    }

    Ok(Instruction::Env { key, value })
}

/// Parse a `RUN <command>` instruction.
///
/// Returns an error if the command string is empty.
fn parse_run(rest: &str, line: usize) -> Result<Instruction, ParseError> {
    let command = rest.trim();

    if command.is_empty() {
        return Err(ParseError::InvalidInstruction {
            line,
            message: "RUN requires a command".to_string(),
        });
    }

    Ok(Instruction::Run {
        command: command.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Inline unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- parse_from ---

    #[test]
    fn parse_from_image_only() {
        let result = parse_from("ubuntu:22.04", 1).expect("should parse image-only FROM");
        assert_eq!(
            result,
            Instruction::From {
                image: "ubuntu:22.04".to_string(),
                alias: None
            }
        );
    }

    #[test]
    fn parse_from_with_alias() {
        let result =
            parse_from("ubuntu:22.04 AS builder", 1).expect("should parse FROM with alias");
        assert_eq!(
            result,
            Instruction::From {
                image: "ubuntu:22.04".to_string(),
                alias: Some("builder".to_string())
            }
        );
    }

    #[test]
    fn parse_from_with_lowercase_as() {
        // The `as` keyword comparison must be case-insensitive.
        let result =
            parse_from("ubuntu:22.04 as builder", 1).expect("lowercase `as` must also be accepted");
        assert_eq!(
            result,
            Instruction::From {
                image: "ubuntu:22.04".to_string(),
                alias: Some("builder".to_string())
            }
        );
    }

    #[test]
    fn parse_from_missing_image_returns_error() {
        let result = parse_from("", 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line 1"));
    }

    #[test]
    fn parse_from_as_without_alias_returns_error() {
        // `FROM ubuntu:22.04 AS` with no alias token must be an error.
        let result = parse_from("ubuntu:22.04 AS", 3);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line 3"));
    }

    // --- parse_workdir ---

    #[test]
    fn parse_workdir_absolute_path() {
        let result = parse_workdir("/app/src", 1).expect("should parse absolute workdir");
        assert_eq!(
            result,
            Instruction::Workdir {
                path: PathBuf::from("/app/src")
            }
        );
    }

    #[test]
    fn parse_workdir_empty_returns_error() {
        let result = parse_workdir("", 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line 5"));
    }

    // --- parse_copy ---

    #[test]
    fn parse_copy_two_args() {
        let result = parse_copy(". /app", 1).expect("should parse COPY with two args");
        assert_eq!(
            result,
            Instruction::Copy {
                source: CopySource::Host(PathBuf::from(".")),
                dest: PathBuf::from("/app"),
            }
        );
    }

    #[test]
    fn parse_copy_one_arg_returns_error() {
        let result = parse_copy("onlyone", 2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line 2"));
    }

    // --- parse_from extra tokens ---

    #[test]
    fn parse_from_extra_token_after_image_returns_error() {
        // `FROM ubuntu extra` — no AS keyword, extra token after image.
        let result = parse_from("ubuntu extra", 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line 1"));
    }

    #[test]
    fn parse_from_extra_token_after_alias_returns_error() {
        // `FROM ubuntu:22.04 AS builder extratoken` — extra token after alias.
        let result = parse_from("ubuntu:22.04 AS builder extratoken", 2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line 2"));
    }

    // --- parse_copy multi-source ---

    #[test]
    fn copy_multi_source_becomes_unknown() {
        // `COPY src1 src2 dest` — multi-source not supported in v0.1.
        // Must become Unknown rather than silently using wrong dest.
        let instructions =
            parse_dockerfile("COPY req.txt pyproject.toml /app/").expect("must not error");
        assert_eq!(instructions.len(), 1);
        assert!(matches!(instructions[0], Instruction::Unknown { .. }));
    }

    // --- parse_env ---

    #[test]
    fn parse_env_equals_form() {
        let result = parse_env("KEY=value", 1).expect("should parse KEY=value");
        assert_eq!(
            result,
            Instruction::Env {
                key: "KEY".to_string(),
                value: "value".to_string()
            }
        );
    }

    #[test]
    fn parse_env_space_form() {
        let result = parse_env("KEY value", 1).expect("should parse KEY VALUE");
        assert_eq!(
            result,
            Instruction::Env {
                key: "KEY".to_string(),
                value: "value".to_string()
            }
        );
    }

    #[test]
    fn parse_env_value_containing_equals() {
        // `ENV URL=http://example.com/a=b` — only the first `=` is the separator.
        let result = parse_env("URL=http://example.com/a=b", 1).expect("should handle = in value");
        assert_eq!(
            result,
            Instruction::Env {
                key: "URL".to_string(),
                value: "http://example.com/a=b".to_string()
            }
        );
    }

    #[test]
    fn parse_env_space_form_preserves_value_whitespace() {
        // The `=` form does not trim the value, so the space form should not
        // either — both branches must behave consistently.
        let result = parse_env("KEY  padded value  ", 1).expect("should parse");
        // key is trimmed (first token), value is everything after first whitespace
        assert_eq!(result, Instruction::Env {
            key: "KEY".to_string(),
            value: " padded value  ".to_string(),
        });
    }

    #[test]
    fn parse_env_empty_value_eq_form() {
        // `ENV KEY=` is valid — the value is an empty string.
        let result = parse_env("KEY=", 1).expect("ENV KEY= should be valid");
        assert_eq!(
            result,
            Instruction::Env {
                key: "KEY".to_string(),
                value: "".to_string()
            }
        );
    }

    // --- parse_run ---

    #[test]
    fn parse_run_with_command() {
        let result = parse_run("echo hello", 1).expect("should parse RUN with command");
        assert_eq!(
            result,
            Instruction::Run {
                command: "echo hello".to_string()
            }
        );
    }

    #[test]
    fn parse_run_empty_returns_error() {
        let result = parse_run("", 4);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line 4"));
    }

    // --- parse_dockerfile (keyword dispatch) ---

    #[test]
    fn keyword_is_case_insensitive() {
        // `from scratch` (all lowercase) must produce Instruction::From.
        let instructions = parse_dockerfile("from scratch").expect("lowercase keyword must parse");
        assert!(matches!(instructions[0], Instruction::From { .. }));
    }

    #[test]
    fn comment_lines_produce_no_instructions() {
        let instructions = parse_dockerfile("# just a comment").expect("comment must not error");
        assert!(instructions.is_empty());
    }

    #[test]
    fn blank_lines_produce_no_instructions() {
        let instructions = parse_dockerfile("   \n\n  ").expect("blanks must not error");
        assert!(instructions.is_empty());
    }

    #[test]
    fn unknown_keyword_produces_unknown_instruction() {
        let instructions = parse_dockerfile("EXPOSE 8080").expect("EXPOSE must not error");
        assert_eq!(instructions.len(), 1);
        assert!(matches!(instructions[0], Instruction::Unknown { .. }));
    }
}
