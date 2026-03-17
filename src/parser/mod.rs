//! Turns Dockerfile and Compose files into typed instruction models.

pub mod dockerfile;
pub mod error;

pub use dockerfile::parse_dockerfile;
pub use error::ParseError;
