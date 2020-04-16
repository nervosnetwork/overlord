use std::fmt::Debug;

use bytes::Bytes;
use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::{Blk, St};

pub type Hash = Bytes;
pub type Address = Bytes;
pub type Signature = Bytes;

pub type PriKeyHex = String;
pub type PubKeyHex = String;
pub type CommonHex = String;

pub type Height = u64;
pub type Round = u64;

pub type Weight = u32;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub enum OverlordMsg<B: Blk> {
    #[display(fmt = "signed_proposal: {}", _0)]
    SignedProposal(SignedProposal<B>),
    #[display(fmt = "signed_Pre_vote: {}", _0)]
    SignedPreVote(SignedPreVote),
    #[display(fmt = "signed_Pre_vote: {}", _0)]
    SignedPreCommit(SignedPreCommit),
    #[display(fmt = "pre_vote_qc: {}", _0)]
    PreVoteQC(PreVoteQC),
    #[display(fmt = "pre_commit_qc: {}", _0)]
    PreCommitQC(PreCommitQC),
    #[display(fmt = "signed_choke: {}", _0)]
    SignedChoke(SignedChoke),
    #[display(fmt = "signed_height: {}", _0)]
    SignedHeight(SignedHeight),
    #[display(fmt = "sync_request: {}", _0)]
    SyncRequest(SyncRequest),
    #[display(fmt = "sync_response: {}", _0)]
    SyncResponse(SyncResponse<B>),
    #[display(fmt = "stop overlord")]
    Stop,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq)]
#[display(
    fmt = "{{ proposal: {}， signature: {} }}",
    proposal,
    "hex::encode(signature)"
)]
pub struct SignedProposal<B: Blk> {
    pub proposal:  Proposal<B>,
    pub signature: Signature,
}

impl<B: Blk> SignedProposal<B> {
    pub fn new(proposal: Proposal<B>, signature: Signature) -> Self {
        SignedProposal {
            proposal,
            signature,
        }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(fmt = "{{ lock_round: {}, lock_votes: {} }}", lock_round, lock_votes)]
pub struct PoLC {
    pub lock_round: Round,
    pub lock_votes: PreVoteQC,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[display(
    fmt = "{{ height: {}, round: {}, block_hash: {} }}",
    height,
    round,
    "hex::encode(block_hash)"
)]
pub struct Vote {
    pub height:     Height,
    pub round:      Round,
    pub block_hash: Hash,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash)]
#[display(
    fmt = "{{ vote: {}, vote_weight: {}, voter: {}, signature: {},  }}",
    vote,
    vote_weight,
    "hex::encode(voter)",
    "hex::encode(signature)"
)]
pub struct SignedPreVote {
    pub vote:        Vote,
    pub vote_weight: Weight,
    pub voter:       Address,
    pub signature:   Signature,
}

impl SignedPreVote {
    pub fn new(vote: Vote, vote_weight: Weight, voter: Address, signature: Signature) -> Self {
        SignedPreVote {
            vote,
            vote_weight,
            voter,
            signature,
        }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash)]
#[display(
    fmt = "{{ vote: {}, vote_weight: {}, voter: {}, signature: {} }}",
    vote,
    vote_weight,
    "hex::encode(voter)",
    "hex::encode(signature)"
)]
pub struct SignedPreCommit {
    pub vote:        Vote,
    pub vote_weight: Weight,
    pub voter:       Address,
    pub signature:   Signature,
}

impl SignedPreCommit {
    pub fn new(vote: Vote, vote_weight: Weight, voter: Address, signature: Signature) -> Self {
        SignedPreCommit {
            vote,
            vote_weight,
            voter,
            signature,
        }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[display(fmt = "{{ vote: {}, aggregates: {} }}", vote, aggregates)]
pub struct PreVoteQC {
    pub vote:       Vote,
    pub aggregates: Aggregates,
}

impl PreVoteQC {
    pub fn new(vote: Vote, aggregates: Aggregates) -> Self {
        PreVoteQC { vote, aggregates }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[display(fmt = "{{ vote: {}, aggregates: {} }}", vote, aggregates)]
pub struct PreCommitQC {
    pub vote:       Vote,
    pub aggregates: Aggregates,
}

impl PreCommitQC {
    pub fn new(vote: Vote, aggregates: Aggregates) -> Self {
        PreCommitQC { vote, aggregates }
    }
}

#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[display(
    fmt = "{{ address_bitmap: {}, signature: {} }}",
    "hex::encode(address_bitmap)",
    "hex::encode(signature)"
)]
pub struct Aggregates {
    pub address_bitmap: Bytes,
    pub signature:      Signature,
}

impl Aggregates {
    pub fn new(address_bitmap: Bytes, signature: Signature) -> Self {
        Aggregates {
            address_bitmap,
            signature,
        }
    }
}

#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Hash)]
#[display(
    fmt = "{{ choke: {}, vote_weight: {}, from: {}, voter: {}, signature: {} }}",
    choke,
    vote_weight,
    from,
    "hex::encode(voter)",
    "hex::encode(signature)"
)]
pub struct SignedChoke {
    pub choke:       Choke,
    pub vote_weight: Weight,
    pub from:        UpdateFrom,
    pub voter:       Address,
    pub signature:   Signature,
}

impl SignedChoke {
    pub fn new(
        choke: Choke,
        vote_weight: Weight,
        from: UpdateFrom,
        voter: Address,
        signature: Signature,
    ) -> Self {
        SignedChoke {
            choke,
            vote_weight,
            from,
            voter,
            signature,
        }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[display(fmt = "{{ height: {}, round: {}}}", height, round)]
pub struct Choke {
    pub height: Height,
    pub round:  Round,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[display(fmt = "{{ aggregates: {}, choke: {} }}", aggregates, choke)]
pub struct ChokeQC {
    pub aggregates: Aggregates,
    pub choke:      Choke,
}

#[derive(Clone, Debug, Display, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpdateFrom {
    #[display(fmt = "UpdateFrom::PreVoteQC ( {} )", _0)]
    PreVoteQC(PreVoteQC),
    #[display(fmt = "UpdateFrom::PreCommitQC ( {} )", _0)]
    PreCommitQC(PreCommitQC),
    #[display(fmt = "UpdateFrom::ChokeQC ( {} )", _0)]
    ChokeQC(ChokeQC),
}

impl Default for UpdateFrom {
    fn default() -> Self {
        UpdateFrom::PreVoteQC(PreVoteQC::default())
    }
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
    fmt = "{{ request_id: {}, request_range: {}, requester: {}, signature: {} }}",
    "hex::encode(request_id)",
    request_range,
    "hex::encode(requester)",
    "hex::encode(signature)"
)]
pub struct SyncRequest {
    pub request_id:    Hash,
    pub request_range: HeightRange,
    pub requester:     Address,
    pub signature:     Signature,
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ request_id: {}, responder: {}, signature: {} }}",
    "hex::encode(request_id)",
    "hex::encode(responder)",
    "hex::encode(signature)"
)]
pub struct SyncResponse<B: Blk> {
    pub request_id:        Hash,
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

impl HeightRange {
    pub fn new(start: Height, number: u64) -> Self {
        HeightRange {
            from: start,
            to:   start + number,
        }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(fmt = "{{ vote: {}, aggregates: {} }}", vote, aggregates)]
pub struct Proof {
    pub vote:       Vote,
    pub aggregates: Aggregates,
}

impl From<PreCommitQC> for Proof {
    fn from(pre_commit_qc: PreCommitQC) -> Self {
        Proof {
            vote:       pre_commit_qc.vote,
            aggregates: pre_commit_qc.aggregates,
        }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq)]
#[display(
    fmt = "{{ height: {}, block_hash: {}}}",
    height,
    "hex::encode(block_hash)"
)]
pub(crate) struct FetchedFullBlock {
    pub(crate) height:     Height,
    pub(crate) block_hash: Hash,
    pub(crate) full_block: Bytes,
}

#[derive(Clone, Debug, Display, Default)]
#[display(
    fmt = "{{ consensus_config: {}, block_states: {} }}",
    consensus_config,
    block_states
)]
pub struct ExecResult<S: St> {
    pub consensus_config: OverlordConfig,
    pub block_states:     BlockState<S>,
}

#[derive(Clone, Debug, Display, Default)]
#[display(fmt = "{{ height: {}, state: {:?} }}", height, state)]
pub struct BlockState<S: St> {
    pub height: Height,
    pub state:  S,
}

impl<S: St> BlockState<S> {
    pub fn new(height: Height, state: S) -> Self {
        BlockState { height, state }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ max_exec_behind: {}, auth_config: {}, timer_config: {} }}",
    max_exec_behind,
    auth_config,
    time_config
)]
pub struct OverlordConfig {
    pub max_exec_behind: u64,
    pub auth_config:     AuthConfig,
    pub time_config:     TimeConfig,
}

#[derive(Clone, Debug, Display, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectMode {
    #[display(fmt = "InTurn")]
    InTurn,
    #[display(fmt = "Random")]
    Random,
}

impl Default for SelectMode {
    fn default() -> Self {
        SelectMode::InTurn
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ common_ref: {}, mode: {}, auth_list: {} }}",
    common_ref,
    mode,
    "DisplayVec(auth_list.clone())"
)]
pub struct AuthConfig {
    pub common_ref: CommonHex,
    pub mode:       SelectMode,
    pub auth_list:  Vec<Node>,
}

#[derive(Clone, Debug, Display, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ interval: {}, propose_ratio: {}, pre_vote_ratio: {}, pre_commit_ratio: {}, brake_ratio: {} }}",
    interval,
    propose_ratio,
    pre_vote_ratio,
    pre_commit_ratio,
    brake_ratio
)]
pub struct TimeConfig {
    pub interval:         u64,
    pub propose_ratio:    u64,
    pub pre_vote_ratio:   u64,
    pub pre_commit_ratio: u64,
    pub brake_ratio:      u64,
}

impl Default for TimeConfig {
    fn default() -> TimeConfig {
        TimeConfig {
            interval:         3000,
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
    pub pub_key:        PubKeyHex,
    pub propose_weight: Weight,
    pub vote_weight:    Weight,
}

impl Node {
    pub fn new(
        address: Address,
        pub_key: PubKeyHex,
        propose_weight: Weight,
        vote_weight: Weight,
    ) -> Self {
        Node {
            address,
            pub_key,
            propose_weight,
            vote_weight,
        }
    }
}

#[derive(Clone, Debug, Display, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteType {
    #[display(fmt = "PreVote")]
    PreVote,
    #[display(fmt = "PreCommit")]
    PreCommit,
    #[display(fmt = "Choke")]
    Choke,
}

impl Default for VoteType {
    fn default() -> Self {
        VoteType::PreVote
    }
}

impl Into<u8> for VoteType {
    fn into(self) -> u8 {
        match self {
            VoteType::PreVote => 0,
            VoteType::PreCommit => 1,
            VoteType::Choke => 2,
        }
    }
}

impl From<u8> for VoteType {
    fn from(s: u8) -> Self {
        match s {
            0 => VoteType::PreVote,
            1 => VoteType::PreCommit,
            2 => VoteType::Choke,
            _ => panic!("Invalid Vote type!"),
        }
    }
}

#[derive(Clone, Debug, Display, Default, PartialEq, Eq)]
#[display(
    fmt = "{{ cum_weight: {}, vote_type: {}, round: {}, block_hash: {} }}",
    cum_weight,
    vote_type,
    round,
    "block_hash.clone().map_or(\"None\".to_owned(), hex::encode)"
)]
pub struct CumWeight {
    pub cum_weight: Weight,
    pub vote_type:  VoteType,
    pub round:      Round,
    pub block_hash: Option<Hash>,
}

impl CumWeight {
    pub fn new(
        cum_weight: Weight,
        vote_type: VoteType,
        round: Round,
        block_hash: Option<Hash>,
    ) -> Self {
        CumWeight {
            cum_weight,
            vote_type,
            round,
            block_hash,
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
