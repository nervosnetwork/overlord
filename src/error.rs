#![allow(dead_code)]
use std::cmp::{Eq, PartialEq};

use derive_more::Display;

/// Overlord consensus error.
#[derive(Debug, Display)]
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
    ///
    #[display(fmt = "Self check not pass {}", _0)]
    SelfCheckErr(String),
    ///
    #[display(fmt = "Correctness error {}", _0)]
    CorrectnessErr(String),
    /// Other error.
    #[display(fmt = "Other error {}", _0)]
    Other(String),
}

impl PartialEq for ConsensusError {
    fn eq(&self, other: &Self) -> bool {
        use self::ConsensusError::*;
        match (self, other) {
            (InvalidAddress, InvalidAddress)
            | (TriggerSMRErr(_), TriggerSMRErr(_))
            | (MonitorEventErr(_), MonitorEventErr(_))
            | (ThrowEventErr(_), ThrowEventErr(_))
            | (ProposalErr(_), ProposalErr(_))
            | (PrevoteErr(_), PrevoteErr(_))
            | (PrecommitErr(_), PrecommitErr(_))
            | (SelfCheckErr(_), SelfCheckErr(_)) => true,
            (RoundDiff { local: m, vote: n }, RoundDiff { local: p, vote: q }) => m == p && n == q,
            (Other(x), Other(y)) | (CorrectnessErr(x), CorrectnessErr(y)) => x == y,
            _ => false,
        }
    }
}

impl Eq for ConsensusError {}
