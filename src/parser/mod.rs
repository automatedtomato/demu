#![allow(dead_code)]

//! Turns Dockerfile and Compose files into typed instruction models.

#[derive(Debug, thiserror::Error)]
#[error("parse error (placeholder — variants added in #3)")]
pub struct ParseError;

#[cfg(test)]
mod tests {}
