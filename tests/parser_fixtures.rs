//! Fixture-based integration tests for the Dockerfile parser.
//!
//! These tests are written BEFORE the implementation (TDD red phase).
//! Each test loads a fixture file and asserts the expected instruction sequence.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use demu::model::instruction::{CopySource, Instruction};
use demu::parser::parse_dockerfile;
use std::path::PathBuf;

#[test]
fn test_minimal() {
    let input = include_str!("fixtures/parser/minimal.dockerfile");
    let instructions = parse_dockerfile(input).expect("should parse minimal dockerfile");
    assert_eq!(instructions.len(), 5);
    assert_eq!(
        instructions[0],
        Instruction::From {
            image: "ubuntu:22.04".to_string(),
            alias: None
        }
    );
    assert_eq!(
        instructions[1],
        Instruction::Workdir {
            path: PathBuf::from("/app")
        }
    );
    assert_eq!(
        instructions[2],
        Instruction::Copy {
            source: CopySource::Host(PathBuf::from(".")),
            dest: PathBuf::from("/app"),
        }
    );
    assert_eq!(
        instructions[3],
        Instruction::Env {
            key: "APP_ENV".to_string(),
            value: "production".to_string()
        }
    );
    assert_eq!(
        instructions[4],
        Instruction::Run {
            command: "echo hello".to_string()
        }
    );
}

#[test]
fn test_env_both_forms() {
    let input = include_str!("fixtures/parser/env_both_forms.dockerfile");
    let instructions = parse_dockerfile(input).expect("should parse env forms");
    // FROM + 2 ENVs = 3 instructions
    assert_eq!(instructions.len(), 3);
    assert_eq!(
        instructions[1],
        Instruction::Env {
            key: "KEY_EQ".to_string(),
            value: "value_eq".to_string()
        }
    );
    assert_eq!(
        instructions[2],
        Instruction::Env {
            key: "KEY_SPACE".to_string(),
            value: "value_space".to_string()
        }
    );
}

#[test]
fn test_unknown_instructions() {
    let input = include_str!("fixtures/parser/unknown_instructions.dockerfile");
    let instructions = parse_dockerfile(input).expect("unknown instructions must not error");
    // FROM + ARG + LABEL + CMD + EXPOSE = 5
    assert_eq!(instructions.len(), 5);
    assert!(matches!(instructions[0], Instruction::From { .. }));
    // Each unknown instruction must preserve the original raw line text so the
    // engine can surface it in history and warnings.
    let raw_lines = [
        "ARG VERSION=1.0",
        "LABEL maintainer=test",
        "CMD [\"/bin/sh\"]",
        "EXPOSE 8080",
    ];
    for (i, expected_raw) in raw_lines.iter().enumerate() {
        match &instructions[i + 1] {
            Instruction::Unknown { raw } => assert_eq!(
                raw,
                expected_raw,
                "instruction[{}] raw text mismatch",
                i + 1
            ),
            other => panic!("expected Unknown at [{}], got {:?}", i + 1, other),
        }
    }
}

#[test]
fn test_comments_and_blanks() {
    let input = include_str!("fixtures/parser/comments_and_blanks.dockerfile");
    let instructions = parse_dockerfile(input).expect("should skip comments and blanks");
    // FROM + WORKDIR + COPY = 3 (2 comments and 2 blanks are dropped)
    assert_eq!(instructions.len(), 3);
    assert_eq!(
        instructions[0],
        Instruction::From {
            image: "scratch".to_string(),
            alias: None
        }
    );
    assert_eq!(
        instructions[1],
        Instruction::Workdir {
            path: PathBuf::from("/app")
        }
    );
    assert_eq!(
        instructions[2],
        Instruction::Copy {
            source: CopySource::Host(PathBuf::from(".")),
            dest: PathBuf::from("/app"),
        }
    );
}

#[test]
fn test_from_with_alias() {
    let input = include_str!("fixtures/parser/from_with_alias.dockerfile");
    let instructions = parse_dockerfile(input).expect("should parse FROM with alias");
    assert_eq!(
        instructions[0],
        Instruction::From {
            image: "ubuntu:22.04".to_string(),
            alias: Some("builder".to_string()),
        }
    );
}

#[test]
fn test_malformed_from_returns_error() {
    let input = include_str!("fixtures/parser/malformed_from.dockerfile");
    let result = parse_dockerfile(input);
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Error message must include "line 1"
    assert!(
        err.to_string().contains("line 1"),
        "error must include line number, got: {err}"
    );
}

#[test]
fn test_malformed_copy_returns_error() {
    let input = include_str!("fixtures/parser/malformed_copy.dockerfile");
    let result = parse_dockerfile(input);
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Error on line 2 (line 1 is FROM scratch)
    assert!(
        err.to_string().contains("line 2"),
        "error must include line number, got: {err}"
    );
}

#[test]
fn test_copy_with_from_flag_becomes_unknown() {
    let input = include_str!("fixtures/parser/copy_with_from_flag.dockerfile");
    let instructions =
        parse_dockerfile(input).expect("COPY --from should not error, becomes Unknown");
    assert_eq!(instructions.len(), 2);
    assert!(matches!(instructions[0], Instruction::From { .. }));
    // COPY --from=builder is unsupported in v0.1 → Unknown
    assert!(matches!(instructions[1], Instruction::Unknown { .. }));
}
