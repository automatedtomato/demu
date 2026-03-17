use crate::model::instruction::Instruction;
use crate::parser::error::ParseError;

/// Parse a Dockerfile string into a sequence of typed instructions.
///
/// This is a stub — the real implementation is in Phase 4.
pub fn parse_dockerfile(_input: &str) -> Result<Vec<Instruction>, ParseError> {
    todo!("implement parser")
}
