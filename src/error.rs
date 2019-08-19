#![allow(dead_code)]

/// Overlord consensus error.
#[derive(Debug, PartialEq, Eq)]
pub enum ConsensusError {
    ///
    InvalidAddress,
    /// Other error.
    Other(String),
}
