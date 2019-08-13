use crate::Codec;

/// Address type.
pub type Address = Vec<u8>;
/// Hash type.
pub type Hash = Vec<u8>;
/// Signature type.
pub type Signature = Vec<u8>;

/// Node roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// The node is a leader.
    Leader = 0,
    /// The node is not a leader.
    Replica = 1,
}

/// Vote of QC types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoteType {
    /// Prevote vote or QC.
    Prevote = 0,
    /// Precommit Vote or QC.
    Precommit = 1,
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
    pub signature: Vec<u8>,
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
    pub lock: Option<(u64, AggregatedVote)>,
    /// Proposer address.
    pub proposer: Address,
}

/// A signed vote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedVote {
    /// Signature of the vote.
    pub signature: Vec<u8>,
    /// A vote.
    pub vote: Vote,
}

/// An aggregrated signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregatedSignature {
    /// Aggregated signature.
    pub signature: Vec<u8>,
    /// Voter address bit map.
    pub address_bitmap: Vec<u8>,
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

/// A verify response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifyResp {
    /// Verified proposal hash.
    pub(crate) epoch_hash: Hash,
    /// The verify result.
    pub(crate) is_pass: bool,
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
