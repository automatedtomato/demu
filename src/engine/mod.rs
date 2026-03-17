#![allow(dead_code)]

//! Applies parsed Dockerfile and Compose instructions into preview state.

#[derive(Debug, thiserror::Error)]
#[error("engine error (placeholder — variants added in #4)")]
pub struct EngineError;
