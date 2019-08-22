use derive_more::Display;

use crate::types::Hash;

/// SMR steps. The default step is commit step because SMR needs rich status to start a new epoch.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// Prepose step, in this step:
    /// Firstly, each node calculate the new proposer, then:
    /// Leader:
    ///     proposer an epoch,
    /// Replica:
    ///     wait for a proposal and check it.
    /// Then goto prevote step.
    Propose = 0,
    /// Prevote step, in this step:
    /// Leader:
    ///     1. wait for others signed prevote votes,
    ///     2. aggregrate them to an aggregrated vote,
    ///     3. broadcast the aggregrated vote to others.
    /// Replica:
    ///     1. transmit prevote vote,
    ///     2. wait for aggregrated vote,
    ///     3. check the aggregrated vote.
    /// Then goto precommit step.
    Prevote = 1,
    /// Precommit step, in this step:
    /// Leader:
    ///     1. wait for others signed precommit votes,
    ///     2. aggregrate them to an aggregrated vote,
    ///     3. broadcast the aggregrated vote to others.
    /// Replica:
    ///     1. transmit precommit vote,
    ///     2. wait for aggregrated vote,
    ///     3. check the aggregrated vote.
    /// If there is no consensus in the precommit step, goto propose step and start a new round
    /// cycle. Otherwise, goto commit step.
    Precommit = 2,
    /// Commit step, in this step each node commit the epoch and wait for the rich status. After
    /// receiving the it, all nodes will goto propose step and start a new epoch consensus.
    Commit = 3,
}

impl Default for Step {
    fn default() -> Self {
        Step::Commit
    }
}

/// SMR event that state and timer monitor this.
#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub enum SMREvent {
    /// New round event,
    /// for state: update round,
    /// for timer: set a propose step timer. If `round == 0`, set an extra total epoch timer.
    #[display(fmt = "New round {} event", round)]
    NewRoundInfo {
        round:         u64,
        lock_round:    Option<u64>,
        lock_proposal: Option<Hash>,
    },
    /// Prevote event,
    /// for state: transmit a prevote vote,
    /// for timer: set a prevote step timer.
    #[display(fmt = "Prevote event")]
    PrevoteVote(Hash),
    /// Precommit event,
    /// for state: transmit a precommit vote,
    /// for timer: set a precommit step timer.
    #[display(fmt = "Precommit event")]
    PrecommitVote(Hash),
    /// Commit event,
    /// for state: do commit,
    /// for timer: do nothing.
    #[display(fmt = "Commit event")]
    Commit(Hash),
}

/// SMR trigger types.
#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub enum TriggerType {
    /// Proposal trigger.
    #[display(fmt = "Proposal")]
    Proposal,
    /// Prevote quorum certificate trigger.
    #[display(fmt = "PrevoteQC")]
    PrevoteQC,
    /// Precommit quorum certificate trigger.
    #[display(fmt = "PrecommitQC")]
    PrecommitQC,
    /// New Epoch trigger.
    #[display(fmt = "New epoch {}", _0)]
    NewEpoch(u64),
}

/// SMR trigger sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerSource {
    /// SMR triggered by state.
    State = 0,
    /// SMR triggered by timer.
    Timer = 1,
}

impl Into<u8> for TriggerType {
    /// It should not occur that call `TriggerType::NewEpoch(*).into()`.
    fn into(self) -> u8 {
        match self {
            TriggerType::Proposal => 0u8,
            TriggerType::PrevoteQC => 1u8,
            TriggerType::PrecommitQC => 2u8,
            TriggerType::NewEpoch(_) => unreachable!(),
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
            3 => unreachable!(),
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
    /// SMR trigger source.
    pub source: TriggerSource,
    /// SMR trigger hash, the meaning shown above.
    pub hash: Hash,
    /// SMR trigger round, the meaning shown above.
    pub round: Option<u64>,
}

/// An inner lock struct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lock {
    /// Lock round.
    pub round: u64,
    /// Lock hash.
    pub hash: Hash,
}