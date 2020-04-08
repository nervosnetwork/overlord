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
    #[display(fmt = "Signed Proposal")]
    SignedProposal(SignedProposal<B>),
    #[display(fmt = "Signed Vote")]
    SignedVote(SignedVote),
    #[display(fmt = "Aggregated Vote")]
    AggregatedVote(AggregatedVote),
    #[display(fmt = "Signed Choke")]
    SignedChoke(SignedChoke),
    #[display(fmt = "New Height")]
    CurrentHeight(Height),
    #[display(fmt = "RequestSync")]
    RequestSync(HeightRange),
    #[display(fmt = "ResponseSync")]
    ResponseSync(Vec<B>),
}

#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[display(fmt = "Signed Proposal {:?}", proposal)]
pub struct SignedProposal<B: Blk> {
    pub signature: Signature,
    pub proposal:  Proposal<B>,
}

#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[display(fmt = "Proposal height {}, round {}", height, round)]
pub struct Proposal<B: Blk> {
    pub height:     Height,
    pub round:      Round,
    pub block:      B,
    pub block_hash: Hash,
    pub lock:       Option<PoLC>,
    pub proposer:   Address,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoLC {
    pub lock_round: Round,
    pub lock_votes: AggregatedVote,
}

#[derive(Clone, Debug, Display, PartialEq, Eq, Hash)]
#[display(fmt = "Signed vote {:?}", vote)]
pub struct SignedVote {
    pub signature: Signature,
    pub vote:      Vote,
    pub voter:     Address,
}

#[derive(Clone, Debug, Display, PartialEq, Eq, Hash)]
#[display(fmt = "{:?} vote height {}, round {}", vote_type, height, round)]
pub struct Vote {
    pub height:     Height,
    pub round:      Round,
    pub vote_type:  VoteType,
    pub block_hash: Hash,
}

#[derive(Serialize, Deserialize, Clone, Debug, Display, PartialEq, Eq, Hash)]
pub enum VoteType {
    #[display(fmt = "Prevote")]
    Prevote,
    #[display(fmt = "Precommit")]
    Precommit,
}

#[derive(Serialize, Deserialize, Clone, Debug, Display, PartialEq, Eq, Hash)]
#[rustfmt::skip]
#[display(fmt = "{:?} aggregated vote height {}, round {}", vote_type, height, round)]
pub struct AggregatedVote {
    pub signature: AggregatedSignature,
    pub vote_type: VoteType,
    pub height: Height,
    pub round: Round,
    pub block_hash: Hash,
    pub leader: Address,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct AggregatedSignature {
    pub signature:      Signature,
    pub address_bitmap: Bytes,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SignedChoke {
    pub signature: Signature,
    pub choke:     Choke,
    pub address:   Address,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Choke {
    pub height: Height,
    pub round:  Round,
    pub from:   UpdateFrom,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub enum UpdateFrom {
    PrevoteQC(AggregatedVote),
    PrecommitQC(AggregatedVote),
    ChokeQC(AggregatedChoke),
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct AggregatedChoke {
    pub height:    Height,
    pub round:     Round,
    pub signature: Signature,
    pub voters:    Vec<Address>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
    pub height:     Height,
    pub round:      Round,
    pub block_hash: Hash,
    pub signature:  AggregatedSignature,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifyResp {
    pub(crate) height:     Height,
    pub(crate) round:      Round,
    pub(crate) block_hash: Hash,
    pub(crate) is_pass:    bool,
}

#[derive(Clone, Debug)]
pub struct ExecResult<S: Clone + Debug> {
    pub consensus_config: ConsensusConfig,
    pub block_states:     BlockState<S>,
}

#[derive(Clone, Debug)]
pub struct BlockState<S: Clone + Debug> {
    pub height: Height,
    pub state:  S,
}

#[derive(Clone, Debug)]
pub struct ConsensusConfig {
    pub interval:        u64,
    pub max_exec_behind: u64,
    pub timer_config:    DurationConfig,
    pub authority_list:  Vec<Node>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DurationConfig {
    pub propose_ratio:   u64,
    pub prevote_ratio:   u64,
    pub precommit_ratio: u64,
    pub brake_ratio:     u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub address:        Address,
    pub propose_weight: u32,
    pub vote_weight:    u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeightRange {
    pub from: Height,
    pub to:   Height,
}
