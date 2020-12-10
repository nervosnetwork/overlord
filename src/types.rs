use std::cmp::{Ord, Ordering, PartialOrd};
use std::convert::TryFrom;

use bytes::Bytes;
use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::error::ConsensusError;
use crate::smr::smr_types::{SMRStatus, Step, TriggerType};
use crate::{Codec, DurationConfig};

/// Address type.
pub type Address = Bytes;
/// Hash type.
pub type Hash = Bytes;
/// Signature type.
pub type Signature = Bytes;

/// Vote or QC types. Prevote and precommit QC will promise the rightness and the final consistency
/// of overlord consensus protocol.
#[derive(Serialize, Deserialize, Clone, Debug, Display, PartialEq, Eq, Hash)]
pub enum VoteType {
    /// Prevote vote or QC.
    #[display(fmt = "Prevote")]
    Prevote,
    /// Precommit Vote or QC.
    #[display(fmt = "Precommit")]
    Precommit,
}

impl Into<u8> for VoteType {
    fn into(self) -> u8 {
        match self {
            VoteType::Prevote => 1,
            VoteType::Precommit => 2,
        }
    }
}

impl Into<TriggerType> for VoteType {
    fn into(self) -> TriggerType {
        match self {
            VoteType::Prevote => TriggerType::PrevoteQC,
            VoteType::Precommit => TriggerType::PrecommitQC,
        }
    }
}

impl Into<Step> for VoteType {
    fn into(self) -> Step {
        match self {
            VoteType::Prevote => Step::Prevote,
            VoteType::Precommit => Step::Precommit,
        }
    }
}

impl TryFrom<u8> for VoteType {
    type Error = ConsensusError;

    fn try_from(s: u8) -> Result<Self, Self::Error> {
        match s {
            1 => Ok(VoteType::Prevote),
            2 => Ok(VoteType::Precommit),
            _ => Err(ConsensusError::Other("".to_string())),
        }
    }
}

/// Overlord messages.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub enum OverlordMsg<T: Codec> {
    /// Signed proposal message.
    #[display(fmt = "Signed Proposal")]
    SignedProposal(SignedProposal<T>),
    /// Signed vote message.
    #[display(fmt = "Signed Vote")]
    SignedVote(SignedVote),
    /// Aggregated vote message.
    #[display(fmt = "Aggregated Vote")]
    AggregatedVote(AggregatedVote),
    /// Rich status message.
    #[display(fmt = "Rich Status")]
    RichStatus(Status),
    /// Signed choke message
    #[display(fmt = "Choke Message")]
    SignedChoke(SignedChoke),
    /// Stop consensus process.
    #[display(fmt = "Stop Overlord")]
    Stop,

    /// This is only for easier testing.
    #[cfg(test)]
    Commit(Commit<T>),
}

impl<T: Codec> OverlordMsg<T> {
    pub(crate) fn is_rich_status(&self) -> bool {
        matches!(self, OverlordMsg::RichStatus(_))
    }

    pub(crate) fn get_height(&self) -> u64 {
        match self {
            OverlordMsg::SignedProposal(sp) => sp.proposal.height,
            OverlordMsg::SignedVote(sv) => sv.get_height(),
            OverlordMsg::AggregatedVote(av) => av.get_height(),
            OverlordMsg::RichStatus(s) => s.height,
            OverlordMsg::SignedChoke(sc) => sc.choke.height,
            _ => unreachable!(),
        }
    }
}

/// How does state goto the current round.
#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub enum UpdateFrom {
    /// From a prevote quorum certificate.
    PrevoteQC(AggregatedVote),
    /// From a precommit quorum certificate.
    PrecommitQC(AggregatedVote),
    /// From a choke quorum certificate.
    ChokeQC(AggregatedChoke),
}

/// The reason of overlord view change.
#[derive(Serialize, Deserialize, Clone, Debug, Display)]
pub enum ViewChangeReason {
    ///
    #[display(fmt = "Do not receive proposal from network")]
    NoProposalFromNetwork,

    ///
    #[display(fmt = "Do not receive Prevote QC from network")]
    NoPrevoteQCFromNetwork,

    ///
    #[display(fmt = "Do not receive precommit QC from network")]
    NoPrecommitQCFromNetwork,

    ///
    #[display(fmt = "Check the block not pass")]
    CheckBlockNotPass,

    ///
    #[display(fmt = "Update from a higher round prevote QC from {} to {}", _0, _1)]
    UpdateFromHigherPrevoteQC(u64, u64),

    ///
    #[display(fmt = "Update from a higher round precommit QC from {} to {}", _0, _1)]
    UpdateFromHigherPrecommitQC(u64, u64),

    ///
    #[display(fmt = "Update from a higher round choke QC from {} to {}", _0, _1)]
    UpdateFromHigherChokeQC(u64, u64),

    ///
    #[display(fmt = "{:?} votes count is below threshold", _0)]
    LeaderReceivedVoteBelowThreshold(VoteType),

    ///
    #[display(fmt = "other reasons")]
    Others,
}

/// A signed proposal.
#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[display(fmt = "Signed Proposal {:?}", proposal)]
pub struct SignedProposal<T: Codec> {
    /// Signature of the proposal.
    pub signature: Bytes,
    /// A proposal.
    pub proposal:  Proposal<T>,
}

/// A proposal
#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[display(fmt = "Proposal height {}, round {}", height, round)]
pub struct Proposal<T: Codec> {
    /// Height of the proposal.
    pub height:     u64,
    /// Round of the proposal.
    pub round:      u64,
    /// Proposal content.
    pub content:    T,
    /// Proposal block hash.
    pub block_hash: Hash,
    /// Optional field. If the proposal has a PoLC, this contains the lock round and lock votes.
    pub lock:       Option<PoLC>,
    /// Proposer address.
    pub proposer:   Address,
}

/// A PoLC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoLC {
    /// Lock round of the proposal.
    pub lock_round: u64,
    /// Lock votes of the proposal.
    pub lock_votes: AggregatedVote,
}

/// A signed vote.
#[derive(Clone, Debug, Display, PartialEq, Eq, Hash)]
#[display(fmt = "Signed vote {:?}", vote)]
pub struct SignedVote {
    /// Signature of the vote.
    pub signature: Bytes,
    /// A vote.
    pub vote:      Vote,
    /// Voter address.
    pub voter:     Address,
}

impl PartialOrd for SignedVote {
    fn partial_cmp(&self, other: &SignedVote) -> Option<Ordering> {
        Some(self.voter.cmp(&other.voter))
    }
}

impl Ord for SignedVote {
    fn cmp(&self, other: &SignedVote) -> Ordering {
        self.voter.cmp(&other.voter)
    }
}

impl SignedVote {
    /// Get the height of the signed vote.
    pub fn get_height(&self) -> u64 {
        self.vote.height
    }

    /// Get the round of the signed vote.
    pub fn get_round(&self) -> u64 {
        self.vote.round
    }

    /// Get the hash of the signed vote.
    pub fn get_hash(&self) -> Hash {
        self.vote.block_hash.clone()
    }

    /// If the signed vote is a prevote vote.
    pub fn is_prevote(&self) -> bool {
        self.vote.vote_type == VoteType::Prevote
    }
}

/// An aggregate signature.
#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct AggregatedSignature {
    /// Aggregated signature.
    #[serde(with = "super::serde_hex")]
    pub signature:      Signature,
    /// Voter address bit map.
    #[serde(with = "super::serde_hex")]
    pub address_bitmap: Bytes,
}

/// An aggregated vote.
#[derive(Serialize, Deserialize, Clone, Debug, Display, PartialEq, Eq, Hash)]
#[rustfmt::skip]
#[display(fmt = "{:?} aggregated vote height {}, round {}", vote_type, height, round)]
pub struct AggregatedVote {
    /// Aggregated signature of the vote.
    pub signature: AggregatedSignature,
    /// Type of the vote.
    pub vote_type: VoteType,
    /// Height of the vote.
    pub height: u64,
    /// Round of the vote.
    pub round: u64,
    /// Proposal hash of the vote.
    #[serde(with = "super::serde_hex")]
    pub block_hash: Hash,
    /// The leader that aggregate the signed votes.
    #[serde(with = "super::serde_hex")]
    pub leader: Address,
}

impl AggregatedVote {
    /// Get the height of the aggregate vote.
    pub fn get_height(&self) -> u64 {
        self.height
    }

    /// Get the round of the aggregate vote.
    pub fn get_round(&self) -> u64 {
        self.round
    }

    /// If the aggregate vote is a prevote quorum certificate.
    pub fn is_prevote_qc(&self) -> bool {
        self.vote_type == VoteType::Prevote
    }

    ///
    pub fn to_vote(&self) -> Vote {
        Vote {
            height:     self.height,
            round:      self.round,
            vote_type:  self.vote_type.clone(),
            block_hash: self.block_hash.clone(),
        }
    }
}

/// A vote.
#[derive(Clone, Debug, Display, PartialEq, Eq, Hash)]
#[display(fmt = "{:?} vote height {}, round {}", vote_type, height, round)]
pub struct Vote {
    /// Height of the vote.
    pub height:     u64,
    /// Round of the vote.
    pub round:      u64,
    /// Type of the vote.
    pub vote_type:  VoteType,
    /// Block hash of the vote.
    pub block_hash: Hash,
}

/// A commit.
#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[display(fmt = "Commit height {}", height)]
pub struct Commit<T: Codec> {
    /// Height of the commit.
    pub height:  u64,
    /// Commit content.
    pub content: T,
    /// The consensus proof.
    pub proof:   Proof,
}

/// A Proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    /// Height of the proof.
    pub height:     u64,
    /// Round of the proof.
    pub round:      u64,
    /// Block hash of the proof.
    pub block_hash: Hash,
    /// Aggregated signature of the proof.
    pub signature:  AggregatedSignature,
}

/// A rich status.
#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[display(fmt = "Rich status height {}", height)]
pub struct Status {
    /// New height.
    pub height:         u64,
    /// New block interval.
    pub interval:       Option<u64>,
    /// New timeout configuration.
    pub timer_config:   Option<DurationConfig>,
    /// New authority list.
    pub authority_list: Vec<Node>,
}

impl Into<SMRStatus> for Status {
    fn into(self) -> SMRStatus {
        SMRStatus {
            height:       self.height,
            new_interval: self.interval,
            new_config:   self.timer_config,
        }
    }
}

impl Status {
    pub(crate) fn is_consensus_node(&self, address: &Address) -> bool {
        self.authority_list
            .iter()
            .any(|node| node.address == address)
    }
}

/// A node info.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Node address.
    #[serde(with = "super::serde_hex")]
    pub address:        Address,
    /// The propose weight of the node. The field is only effective in `features =
    /// "random_leader"`.
    pub propose_weight: u32,
    /// The vote weight of the node.
    pub vote_weight:    u32,
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Node) -> Option<Ordering> {
        Some(self.address.cmp(&other.address))
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Node) -> Ordering {
        self.address.cmp(&other.address)
    }
}

impl Node {
    /// Create a new node with defaule propose weight `1` and vote weight `1`.
    pub fn new(addr: Address) -> Self {
        Node {
            address:        addr,
            propose_weight: 1u32,
            vote_weight:    1u32,
        }
    }

    /// Set a new propose weight of the node. Propose weight is only effective in `features =
    /// "random_leader"`.
    pub fn set_propose_weight(&mut self, propose_weight: u32) {
        self.propose_weight = propose_weight;
    }

    /// Set a new vote weight of the node.
    pub fn set_vote_weight(&mut self, vote_weight: u32) {
        self.vote_weight = vote_weight;
    }
}

/// A verify response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifyResp {
    /// The height of the verified block.
    pub(crate) height:     u64,
    /// The round of the verified block.
    pub(crate) round:      u64,
    /// Verified proposal hash.
    pub(crate) block_hash: Hash,
    /// The block is pass or not.
    pub(crate) is_pass:    bool,
}

/// An aggregated choke.
#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct AggregatedChoke {
    /// The height of the aggregated choke.
    pub height:    u64,
    /// The round of the aggregated choke.
    pub round:     u64,
    /// The aggregated signature of the aggregated choke.
    #[serde(with = "super::serde_hex")]
    pub signature: Signature,
    /// The voters of the aggregated choke.
    #[serde(with = "super::serde_multi_hex")]
    pub voters:    Vec<Address>,
}

#[allow(clippy::len_without_is_empty)]
impl AggregatedChoke {
    pub(crate) fn len(&self) -> usize {
        self.voters.len()
    }

    pub(crate) fn to_hash(&self) -> HashChoke {
        HashChoke {
            height: self.height,
            round:  self.round,
        }
    }
}

/// A signed choke.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SignedChoke {
    /// The signature of the choke.
    pub signature: Signature,
    /// The choke message.
    pub choke:     Choke,
    /// The choke address.
    pub address:   Address,
}

/// A choke.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Choke {
    /// The height of the choke.
    pub height: u64,
    /// The round of the choke.
    pub round:  u64,
    /// How does state goto the current round.
    pub from:   UpdateFrom,
}

impl Choke {
    pub(crate) fn to_hash(&self) -> HashChoke {
        HashChoke {
            height: self.height,
            round:  self.round,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HashChoke {
    pub(crate) height: u64,
    pub(crate) round:  u64,
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::random;

    fn gen_address() -> Address {
        Address::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>())
    }

    fn mock_node() -> Node {
        Node::new(gen_address())
    }

    fn mock_status() -> Status {
        Status {
            height:         random::<u64>(),
            interval:       None,
            timer_config:   None,
            authority_list: vec![mock_node(), mock_node()],
        }
    }

    #[test]
    fn test_consensus_power() {
        let status = mock_status();
        let consensus_node = status.authority_list[0].address.clone();
        let sync_node = gen_address();

        assert!(status.is_consensus_node(&consensus_node));
        assert!(!status.is_consensus_node(&sync_node));
    }
}
