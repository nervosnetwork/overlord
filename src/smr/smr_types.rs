use crate::types::Hash;

/// SMR event that state and timer monitor this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SMREvent {
    /// New round event,
    /// for state: update round,
    /// for timer: set a propose step timer. If `round == 0`, set an extra total epoch timer.
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
    /// Commit event,
    /// for state: do commit,
    /// for timer: do nothing.
    Commit(Hash),
}

/// SMR trigger types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerType {
    /// Proposal trigger.
    Proposal,
    /// Prevote quorum certificate trigger.
    PrevoteQC,
    /// Precommit quorum certificate trigger.
    PrecommitQC,
    /// New Epoch trigger.
    NewEpoch(u64),
}

impl Into<u8> for TriggerType {
    /// It should not occur that call `TriggerType::NewEpoch(*).into()`.
    fn into(self) -> u8 {
        match self {
            TriggerType::Proposal => 0u8,
            TriggerType::PrevoteQC => 1u8,
            TriggerType::PrecommitQC => 2u8,
            TriggerType::NewEpoch(_) => 3u8,
        }
    }
}

impl From<u8> for TriggerType {
    /// It should not occur that call `from(3u8)`.
    fn from(s: u8) -> Self {
        match s {
            0 => TriggerType::Proposal,
            1 => TriggerType::PrevoteQC,
            2 => TriggerType::PrecommitQC,
            3 => TriggerType::NewEpoch(u64::max_value()),
            _ => panic!("Invalid trigger type!"),
        }
    }
}

/// A SMR trigger to touch off SMR process. For different trigger type,
/// the field `hash` and `round` have different restrictions and meaning.
/// While trigger type is `Proposal`:
///     * `hash`: Proposal epoch hash,
///     * `round`: Optional lock round.
/// While trigger type is `PrevoteQC` or `PrecommitQC`:
///     * `hash`: QC epoch hash,
///     * `round`: QC round, this must be `Some`.
/// While trigger type is `NewEpoch`:
///     * `hash`: A empty hash,
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
