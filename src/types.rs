use bytes::Bytes;

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// The node is a leader.
    Leader,
    /// The node is not a leader.
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoteType {
    /// Prevote vote or QC.
    Prevote,
    /// Precommit Vote or QC.
    Precommit,
}

impl Into<u8> for VoteType {
    fn into(self) -> u8 {
        match self {
            VoteType::Prevote => 0,
            VoteType::Precommit => 1,
        }
    }
}

impl From<u8> for VoteType {
    fn from(s: u8) -> Self {
        match s {
            0 => VoteType::Prevote,
            1 => VoteType::Precommit,
            _ => panic!("Invalid vote type!"),
        }
    }
}

/// Overlord output messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputMsg<T: Codec> {
    /// Signed proposal message.
    SignedProposal(SignedProposal<T>),
    /// Signed vote message.
    SignedVote(SignedVote),
    /// Aggregated vote message.
    AggregatedVote(AggregatedVote),
}

/// A signed proposal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedProposal<T: Codec> {
    /// Signature of the proposal.
    pub signature: Bytes,
    /// A proposal.
    pub proposal: Proposal<T>,
}

/// A proposal
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal<T: Codec> {
    /// Epoch ID of the proposal.
    pub epoch_id: u64,
    /// Round of the proposal.
    pub round: u64,
    /// Proposal content.
    pub content: T,
    /// Proposal epoch hash.
    pub epoch_hash: Hash,
    /// Optional field. If the proposal has a PoLC, this contains the lock round and lock votes.
    pub lock: Option<PoLC>,
    /// Proposer address.
    pub proposer: Address,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedVote {
    /// Signature of the vote.
    pub signature: Bytes,
    /// A vote.
    pub vote: Vote,
}

/// An aggregrated signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregatedSignature {
    /// Aggregated signature.
    pub signature: Bytes,
    /// Voter address bit map.
    pub address_bitmap: Bytes,
}

/// An aggregrated vote.
#[derive(Clone, Debug, PartialEq, Eq)]
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
}

/// A vote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vote {
    /// Epoch ID of the vote.
    pub epoch_id: u64,
    /// Round of the vote.
    pub round: u64,
    /// Type of the vote.
    pub vote_type: VoteType,
    /// Epoch hash of the vote.
    pub epoch_hash: Hash,
    /// Voter address.
    pub voter: Address,
}

/// A commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit<T: Codec> {
    /// Epoch ID of the commit.
    pub epoch_id: u64,
    /// Commit content.
    pub content: T,
    /// The consensus proof.
    pub proof: Proof,
}

/// A Proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    /// Epoch ID of the proof.
    pub epoch_id: u64,
    /// Round of the proof.
    pub round: u64,
    /// Epoch hash of the proof.
    pub epoch_hash: Hash,
    /// Aggregated signature of the proof.
    pub signature: AggregatedSignature,
}

/// A rich status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// New epoch ID.
    pub epoch_id: u64,
    /// New block interval.
    pub interval: u64,
    /// New authority list.
    pub authority_list: Vec<Node>,
}

/// A node info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Node address.
    pub address: Address,
    /// The propose weight of the node.
    pub proposal_weight: usize,
    /// The vote weight of the node.
    pub vote_weight: usize,
}

/// A feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Feed<T: Codec> {
    /// Epoch ID of the proposal.
    pub(crate) epoch_id: u64,
    /// Feed content.
    pub(crate) content: T,
    /// The epoch hash.
    pub(crate) epoch_hash: Hash,
}

/// A verify response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifyResp {
    /// Verified proposal hash.
    pub(crate) epoch_hash: Hash,
    /// The verify result.
    pub(crate) is_pass: bool,
}
