#![allow(dead_code)]

/// Overlord consensus error.
#[derive(Debug, PartialEq, Eq)]
pub enum ConsensusError {
    ///
    InvalidAddress,
    ///
    TriggerSMRErr(u8),
    ///
    MonitorEventErr(String),
    /// Other error.
    Other(String),
}
