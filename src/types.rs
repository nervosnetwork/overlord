use std::cmp::{Ord, Ordering, PartialOrd};

use bytes::Bytes;
use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::smr::smr_types::TriggerType;
use crate::Codec;

/// Address type.
pub type Address = Bytes;
/// Hash type.
pub type Hash = Bytes;
/// Signature type.
pub type Signature = Bytes;

/// There are three roles in overlord consensus protocol, leader, relayer and others. Leader needs
/// to propose proposal in a round to propel consensus process. Relayer is the node that responsible
/// to aggregate vote. The others node only vote for a proposal and receive QCs. To simplify the
/// process, the leader and the relayer will be a same node which means leader will alse do what
/// relayer node do.
#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub enum Role {
    /// The node is a leader.
    #[display(fmt = "Leader")]
    Leader,
    /// The node is not a leader.
    #[display(fmt = "Replica")]
    Replica,
}

impl Into<u8> for Role {
    fn into(self) -> u8 {
        match self {
            Role::Leader => 0,
            Role::Replica => 1,
        }
    }
}

impl From<u8> for Role {
    fn from(s: u8) -> Self {
        match s {
            0 => Role::Leader,
            1 => Role::Replica,
            _ => panic!("Invalid role!"),
        }
    }
}

/// Vote or QC types. Prevote and precommit QC will promise the rightness and the final consistency
/// of overlord consensus protocol.
#[derive(Clone, Debug, Display, PartialEq, Eq, Hash)]
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

impl From<u8> for VoteType {
    fn from(s: u8) -> Self {
        match s {
            1 => VoteType::Prevote,
            2 => VoteType::Precommit,
            _ => panic!("Invalid vote type!"),
        }
    }
}

/// Overlord messages.
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

    /// This is only for easier testing.
    #[cfg(test)]
    Commit(Commit<T>),
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
#[display(fmt = "Proposal epoch ID {}, round {}", epoch_id, round)]
pub struct Proposal<T: Codec> {
    /// Epoch ID of the proposal.
    pub epoch_id:   u64,
    /// Round of the proposal.
    pub round:      u64,
    /// Proposal content.
    pub content:    T,
    /// Proposal epoch hash.
    pub epoch_hash: Hash,
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

impl SignedVote {
    /// Get the epoch ID of the signed vote.
    pub fn get_epoch(&self) -> u64 {
        self.vote.epoch_id
    }

    /// Get the round of the signed vote.
    pub fn get_round(&self) -> u64 {
        self.vote.round
    }

    /// Get the hash of the signed vote.
    pub fn get_hash(&self) -> Hash {
        self.vote.epoch_hash.clone()
    }

    /// If the signed vote is a prevote vote.
    pub fn is_prevote(&self) -> bool {
        self.vote.vote_type == VoteType::Prevote
    }
}

/// An aggregate signature.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AggregatedSignature {
    /// Aggregated signature.
    pub signature:      Signature,
    /// Voter address bit map.
    pub address_bitmap: Bytes,
}

/// An aggregated vote.
#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[rustfmt::skip]
#[display(fmt = "{:?} aggregated vote epoch ID {}, round {}", vote_type, epoch_id, round)]
pub struct AggregatedVote {
    /// Aggregated signature of the vote.
    pub signature: AggregatedSignature,
    /// Type of the vote.
    pub vote_type: VoteType,
    /// Epoch ID of the vote.
    pub epoch_id: u64,
    /// Round of the vote.
    pub round: u64,
    /// Proposal hash of the vote.
    pub epoch_hash: Hash,
    /// The leader that aggregate the signed votes.
    pub leader: Address,
}

impl AggregatedVote {
    /// Get the epoch ID of the aggregate vote.
    pub fn get_epoch(&self) -> u64 {
        self.epoch_id
    }

    /// Get the round of the aggregate vote.
    pub fn get_round(&self) -> u64 {
        self.round
    }

    /// If the aggregate vote is a prevote quorum certificate.
    pub fn is_prevote_qc(&self) -> bool {
        self.vote_type == VoteType::Prevote
    }
}

/// A vote.
#[derive(Clone, Debug, Display, PartialEq, Eq, Hash)]
#[display(fmt = "{:?} vote epoch ID {}, round {}", vote_type, epoch_id, round)]
pub struct Vote {
    /// Epoch ID of the vote.
    pub epoch_id:   u64,
    /// Round of the vote.
    pub round:      u64,
    /// Type of the vote.
    pub vote_type:  VoteType,
    /// Epoch hash of the vote.
    pub epoch_hash: Hash,
}

/// A commit.
#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[display(fmt = "Commit epoch ID {}", epoch_id)]
pub struct Commit<T: Codec> {
    /// Epoch ID of the commit.
    pub epoch_id: u64,
    /// Commit content.
    pub content:  T,
    /// The consensus proof.
    pub proof:    Proof,
}

/// A Proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    /// Epoch ID of the proof.
    pub epoch_id:   u64,
    /// Round of the proof.
    pub round:      u64,
    /// Epoch hash of the proof.
    pub epoch_hash: Hash,
    /// Aggregated signature of the proof.
    pub signature:  AggregatedSignature,
}

/// A rich status.
#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[display(fmt = "Rich status epoch ID {}", epoch_id)]
pub struct Status {
    /// New epoch ID.
    pub epoch_id:       u64,
    /// New block interval.
    pub interval:       Option<u64>,
    /// New authority list.
    pub authority_list: Vec<Node>,
}

/// A node info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Node address.
    pub address:        Address,
    /// The propose weight of the node.
    pub propose_weight: u8,
    /// The vote weight of the node.
    pub vote_weight:    u8,
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
            propose_weight: 1u8,
            vote_weight:    1u8,
        }
    }

    /// Set a new propose weight of the node.
    pub fn set_propose_weight(&mut self, propose_weight: u8) {
        self.propose_weight = propose_weight;
    }

    /// Set a new vote weight of the node.
    pub fn set_vote_weight(&mut self, vote_weight: u8) {
        self.vote_weight = vote_weight;
    }
}

/// A feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Feed<T: Codec> {
    /// Epoch ID of the proposal.
    pub(crate) epoch_id:   u64,
    /// Feed content.
    pub(crate) content:    T,
    /// The epoch hash.
    pub(crate) epoch_hash: Hash,
}

/// A verify response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyResp<T: Codec> {
    /// The epoch ID of the verified epoch.
    pub epoch_id:   u64,
    /// Verified proposal hash.
    pub epoch_hash: Hash,
    /// If the verify result is passed, this field is `Some`, otherwise it is `None`.
    pub full_txs:   Option<T>,
}
