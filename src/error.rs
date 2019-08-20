#![allow(dead_code)]
use derive_more::Display;

/// Overlord consensus error.
#[derive(Debug, Display, PartialEq, Eq)]
pub enum ConsensusError {
    ///
    #[display(fmt = "Invalid address")]
    InvalidAddress,
    ///
    #[display(fmt = "Trigger {} SMR error", _0)]
    TriggerSMRErr(String),
    ///
    #[display(fmt = "Monitor {} event error", _0)]
    MonitorEventErr(String),
    ///
    #[display(fmt = "Throw {} event error", _0)]
    ThrowEventErr(String),
    ///
    #[display(fmt = "Proposal error {}", _0)]
    ProposalErr(String),
    ///
    #[display(fmt = "Prevote error {}", _0)]
    PrevoteErr(String),
    ///
    #[display(fmt = "Precommit error {}", _0)]
    PrecommitErr(String),
    ///
    #[display(fmt = "Self round is {}, vote round is {}", local, vote)]
    RoundDiff { local: u64, vote: u64 },
    /// Other error.
    #[display(fmt = "Other error {}", _0)]
    Other(String),
}
