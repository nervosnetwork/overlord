use std::fmt::Debug;

use bytes::Bytes;
use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::Blk;

pub type Hash = Bytes;
pub type Address = Bytes;
pub type Signature = Bytes;

pub type Height = u64;
pub type Round = u64;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub enum OverlordMsg<B: Blk> {
    #[display(fmt = "signed_proposal: {}", _0)]
    SignedProposal(SignedProposal<B>),
    #[display(fmt = "signed_vote: {}", _0)]
    SignedVote(SignedVote),
    #[display(fmt = "aggregated_vote: {}", _0)]
    AggregatedVote(AggregatedVote),
    #[display(fmt = "signed_choke: {}", _0)]
    SignedChoke(SignedChoke),
    #[display(fmt = "current_height: {}", _0)]
    SignedHeight(SignedHeight),
    #[display(fmt = "request_sync: {:?}", _0)]
    SyncRequest(SyncRequest),
    #[display(fmt = "response_sync")]
    SyncResponse(SyncResponse<B>),
    #[display(fmt = "stop overlord")]
    Stop,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq)]
#[display(
    fmt = "{{ signature: {}, proposal: {} }}",
    "hex::encode(signature)",
    proposal
)]
pub struct SignedProposal<B: Blk> {
    pub signature: Signature,
    pub proposal:  Proposal<B>,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq)]
#[display(
    fmt = "{{ height: {}, round: {}, block_hash: {}, lock: {}, proposer: {} }}",
    height,
    round,
    "hex::encode(block_hash)",
    "lock.clone().map_or(\"None\".to_owned(), |polc| format!(\"{}\", polc))",
    "hex::encode(proposer)"
)]
pub struct Proposal<B: Blk> {
    pub height:     Height,
    pub round:      Round,
    pub block:      B,
    pub block_hash: Hash,
    pub lock:       Option<PoLC>,
    pub proposer:   Address,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq)]
#[display(fmt = "{{ lock_round: {}, lock_votes: {} }}", lock_round, lock_votes)]
pub struct PoLC {
    pub lock_round: Round,
    pub lock_votes: AggregatedVote,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash)]
#[display(
    fmt = "{{ signature: {}, vote: {}, voter: {} }}",
    "hex::encode(signature)",
    vote,
    "hex::encode(voter)"
)]
pub struct SignedVote {
    pub signature: Signature,
    pub vote:      Vote,
    pub voter:     Address,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash)]
#[display(
    fmt = "{{ height: {}, round: {}, vote_type: {}, block_hash: {} }}",
    height,
    round,
    vote_type,
    "hex::encode(block_hash)"
)]
pub struct Vote {
    pub height:     Height,
    pub round:      Round,
    pub vote_type:  VoteType,
    pub block_hash: Hash,
}

#[derive(Clone, Debug, Display, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VoteType {
    #[display(fmt = "PreVote")]
    PreVote,
    #[display(fmt = "PreCommit")]
    PreCommit,
}

impl Default for VoteType {
    fn default() -> Self {
        VoteType::PreVote
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[display(
    fmt = "{{ agg_signature: {}, vote_type: {}, height: {}, round: {}, block_hash: {}, leader: {} }}",
    agg_signature,
    vote_type,
    height,
    round,
    "hex::encode(block_hash)",
    "hex::encode(leader)"
)]
pub struct AggregatedVote {
    pub agg_signature: AggregatedSignature,
    pub vote_type:     VoteType,
    pub height:        Height,
    pub round:         Round,
    pub block_hash:    Hash,
    pub leader:        Address,
}

#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[display(
    fmt = "{{ signature: {}, address_bitmap: {} }}",
    "hex::encode(signature)",
    "hex::encode(address_bitmap)"
)]
pub struct AggregatedSignature {
    pub signature:      Signature,
    pub address_bitmap: Bytes,
}

#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Hash)]
#[display(
    fmt = "{{ signature: {}, choke: {}, address: {} }}",
    "hex::encode(signature)",
    choke,
    "hex::encode(address)"
)]
pub struct SignedChoke {
    pub signature: Signature,
    pub choke:     Choke,
    pub address:   Address,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash)]
#[display(fmt = "{{ height: {}, round: {}, from: {} }}", height, round, from)]
pub struct Choke {
    pub height: Height,
    pub round:  Round,
    pub from:   UpdateFrom,
}

#[derive(Clone, Debug, Display, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpdateFrom {
    #[display(fmt = "UpdateFrom::PreVoteQC ( {} )", _0)]
    PreVoteQC(AggregatedVote),
    #[display(fmt = "UpdateFrom::PreCommitQC ( {} )", _0)]
    PreCommitQC(AggregatedVote),
    #[display(fmt = "UpdateFrom::ChokeQC ( {} )", _0)]
    ChokeQC(AggregatedChoke),
}

impl Default for UpdateFrom {
    fn default() -> Self {
        UpdateFrom::PreVoteQC(AggregatedVote::default())
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[display(
    fmt = "{{ height: {}, round: {}, signature: {}, voters: {} }}",
    height,
    round,
    "hex::encode(signature)",
    "DisplayVec(voters.iter().map(hex::encode).collect::<Vec<String>>())"
)]
pub struct AggregatedChoke {
    pub height:    Height,
    pub round:     Round,
    pub signature: Signature,
    pub voters:    Vec<Address>,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ height: {}, address: {}, signature: {} }}",
    height,
    "hex::encode(address)",
    "hex::encode(signature)"
)]
pub struct SignedHeight {
    pub height:    Height,
    pub address:   Address,
    pub signature: Signature,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ request_range: {}, requester: {}, signature: {} }}",
    request_range,
    "hex::encode(requester)",
    "hex::encode(signature)"
)]
pub struct SyncRequest {
    pub request_range: HeightRange,
    pub requester:     Address,
    pub signature:     Signature,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ response_range: {}, responder: {}, signature: {} }}",
    response_range,
    "hex::encode(responder)",
    "hex::encode(signature)"
)]
pub struct SyncResponse<B: Blk> {
    pub response_range:    HeightRange,
    pub block_with_proofs: Vec<(B, Proof)>,
    pub responder:         Address,
    pub signature:         Signature,
}

#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Serialize, Deserialize)]
#[display(fmt = "{{ from: {}, to: {} }}", from, to)]
pub struct HeightRange {
    pub from: Height,
    pub to:   Height,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ height: {}, round: {}, block_hash: {}, agg_signature: {} }}",
    height,
    round,
    "hex::encode(block_hash)",
    agg_signature
)]
pub struct Proof {
    pub height:        Height,
    pub round:         Round,
    pub block_hash:    Hash,
    pub agg_signature: AggregatedSignature,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq)]
#[display(
    fmt = "{{ height: {}, round: {}, block_hash: {}, is_pass: {} }}",
    height,
    round,
    "hex::encode(block_hash)",
    is_pass
)]
pub(crate) struct VerifyResp {
    pub(crate) height:     Height,
    pub(crate) round:      Round,
    pub(crate) block_hash: Hash,
    pub(crate) is_pass:    bool,
}

#[derive(Clone, Debug, Display, Default)]
#[display(
    fmt = "{{ consensus_config: {}, block_states: {} }}",
    consensus_config,
    block_states
)]
pub struct ExecResult<S: Clone + Debug + Default> {
    pub consensus_config: ConsensusConfig,
    pub block_states:     BlockState<S>,
}

#[derive(Clone, Debug, Display, Default)]
#[display(fmt = "{{ height: {}, state: {:?} }}", height, state)]
pub struct BlockState<S: Clone + Debug + Default> {
    pub height: Height,
    pub state:  S,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ interval: {}, max_exec_behind: {}, timer_config: {}, authority_list: {} }}",
    interval,
    max_exec_behind,
    timer_config,
    "DisplayVec(authority_list.clone())"
)]
pub struct ConsensusConfig {
    pub interval:        u64,
    pub max_exec_behind: u64,
    pub timer_config:    DurationConfig,
    pub authority_list:  Vec<Node>,
}

#[derive(Clone, Debug, Display, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ propose_ratio: {}, pre_vote_ratio: {}, pre_commit_ratio: {}, brake_ratio: {} }}",
    propose_ratio,
    pre_vote_ratio,
    pre_commit_ratio,
    brake_ratio
)]
pub struct DurationConfig {
    pub propose_ratio:    u64,
    pub pre_vote_ratio:   u64,
    pub pre_commit_ratio: u64,
    pub brake_ratio:      u64,
}

impl Default for DurationConfig {
    fn default() -> DurationConfig {
        DurationConfig {
            propose_ratio:    15,
            pre_vote_ratio:   10,
            pre_commit_ratio: 7,
            brake_ratio:      10,
        }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ address: {}, propose_w: {}, vote_w: {} }}",
    "hex::encode(address)",
    propose_weight,
    vote_weight
)]
pub struct Node {
    pub address:        Address,
    pub propose_weight: u32,
    pub vote_weight:    u32,
}

impl Node {
    pub fn new(address: Address) -> Self {
        Node {
            address,
            propose_weight: 1,
            vote_weight: 1,
        }
    }
}

struct DisplayVec<T: std::fmt::Display>(Vec<T>);

impl<T: std::fmt::Display> std::fmt::Display for DisplayVec<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "[ ")?;
        for el in &self.0 {
            write!(f, "{}, ", el)?;
        }
        write!(f, "]")?;
        Ok(())
    }
}
