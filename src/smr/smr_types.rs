#![allow(dead_code)]
use crate::types::Hash;

/// SMR event that state and timer monitor this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SMREvent {
    /// New round event
    /// for state: update round,
    /// for timer: set a propose step timer.
    NewRoundInfo {
        round:         u64,
        lock_round:    Option<u64>,
        lock_proposal: Option<Hash>,
    },
    /// Prevote event,
    /// for state: transmit a prevote vote,
    /// for timer: set a prevote step timer.
    PrevoteVote(Hash),
    /// Precommit event,
    /// for state: transmit a precommit vote,
    /// for timer: set a precommit step timer.
    PrecommitVote(Hash),
    /// Commit event
    /// for state: do commit,
    /// for timer: do nothing.
    Commit(Hash),
}

/// SMR trigger types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerType {
    /// Proposal trigger.
    Proposal = 0,
    /// Prevote quorum certificate trigger.
    PrevoteQC = 1,
    /// Precommit quorum certificate trigger.
    PrecommitQC = 2,
    /// Verify response trigger.
    VerifyResp = 3,
}

/// A SMR trigger to touch off SMR process. For different trigger type,
/// the field `hash` and `round` have different restrictions and meaning.
/// While trigger type is `Proposal`:
///     * `hash`: Proposal epoch hash,
///     * `round`: Optional lock round.
/// While trigger type is `PrevoteQC` or `PrecommitQC`:
///     * `hash`: QC epoch hash,
///     * `round`: QC round, this must be `Some`.
/// While trigger type is `VerifyResp':
///     * `hash`: The verified failed epoch hash,
///     * `round`: This must be `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SMRTrigger {
    /// SMR trigger type.
    pub trigger_type: TriggerType,
    /// SMR trigger hash, the meaning shown above.
    pub hash: Hash,
    /// SMR trigger round, the meaning shown above.
    pub round: Option<u64>,
}
