#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]

use std::cmp::{Ord, Ordering, PartialOrd};
use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;

use bytes::Bytes;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::UnboundedReceiver;
use rlp::{decode, encode, DecoderError};

use crate::auth::AuthManage;
use crate::cabinet::Cabinet;
use crate::smr::EventAgent;
use crate::types::{
    Choke, ChokeQC, PreCommitQC, PreVoteQC, Proposal, SignedChoke, SignedPreCommit, SignedPreVote,
    SignedProposal, UpdateFrom,
};
use crate::{
    Adapter, Address, Blk, BlockState, CommonHex, Hash, Height, OverlordConfig, OverlordError,
    OverlordMsg, OverlordResult, PriKeyHex, Proof, Round, St, TimeConfig, Wal, INIT_ROUND,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Display, Default, Eq, PartialEq)]
#[display(
    fmt = "stage: {}, lock: {}, pre_commit_qc: {}, from: {}",
    stage,
    "lock.clone().map_or(\"None\".to_owned(), |lock| format!(\"{}\", lock))",
    "pre_commit_qc.clone().map_or(\"None\".to_owned(), |qc| format!(\"{}\", qc))",
    "from.clone().map_or(\"None\".to_owned(), |from| format!(\"{}\", from))"
)]
pub struct StateInfo<B: Blk> {
    // current info
    pub stage: Stage,

    pub lock:          Option<PreVoteQC>,
    pub block:         Option<B>,
    pub pre_commit_qc: Option<PreCommitQC>,
    pub from:          Option<UpdateFrom>,
}

impl<B: Blk> StateInfo<B> {
    pub fn from_height(height: Height) -> Self {
        StateInfo {
            stage:         Stage::new(height, INIT_ROUND, Step::Propose),
            lock:          None,
            pre_commit_qc: None,
            block:         None,
            from:          None,
        }
    }

    pub fn next_height(&mut self) {
        self.stage.next_height();
        self.lock = None;
        self.pre_commit_qc = None;
        self.block = None;
        self.from = None;
    }

    pub fn handle_signed_proposal(&mut self, sp: &SignedProposal<B>) -> OverlordResult<()> {
        let next_stage = self.filter_stage(sp)?;
        if next_stage.round > self.stage.round {
            let qc = sp
                .proposal
                .lock
                .clone()
                .expect("Unreachable! Have checked lock exists before");
            self.handle_pre_vote_qc(&qc, sp.proposal.block.clone())?;
            return Err(OverlordError::debug_high());
        }
        if self.stage.update_stage(next_stage) {
            self.from = Some(UpdateFrom::PreVoteQC(
                sp.proposal
                    .lock
                    .clone()
                    .expect("Unreachable! Have checked lock exists before"),
            ));
        }
        self.update_lock(&sp.proposal)?;
        Ok(())
    }

    pub fn handle_pre_vote_qc(&mut self, pre_vote_qc: &PreVoteQC, block: B) -> OverlordResult<()> {
        let next_stage = self.filter_stage(pre_vote_qc)?;
        if self.stage.update_stage(next_stage) {
            self.from = Some(UpdateFrom::PreVoteQC(pre_vote_qc.clone()));
        }
        self.lock = Some(pre_vote_qc.clone());
        self.block = Some(block);
        Ok(())
    }

    pub fn handle_pre_commit_qc(
        &mut self,
        pre_commit_qc: &PreCommitQC,
        block: B,
    ) -> OverlordResult<()> {
        let next_stage = self.filter_stage(pre_commit_qc)?;
        if self.stage.update_stage(next_stage) {
            self.from = Some(UpdateFrom::PreCommitQC(pre_commit_qc.clone()));
        }
        self.pre_commit_qc = Some(pre_commit_qc.clone());
        Ok(())
    }

    pub fn from_wal(wal: &Wal) -> OverlordResult<Self> {
        let encode = wal.load_state()?;
        decode(&encode).map_err(OverlordError::local_decode)
    }

    pub fn save_wal(&self, wal: &Wal) -> OverlordResult<()> {
        let encode = encode(self);
        wal.save_state(&Bytes::from(encode))
    }

    pub fn filter_stage<T: NextStage>(&self, msg: &T) -> OverlordResult<Stage> {
        let next_stage = msg.next_stage();
        if next_stage <= self.stage {
            return Err(OverlordError::debug_under_stage());
        }
        Ok(next_stage)
    }

    pub fn update_lock(&mut self, proposal: &Proposal<B>) -> OverlordResult<()> {
        if let Some(qc) = &proposal.lock {
            if let Some(lock) = &self.lock {
                if qc.vote.round > lock.vote.round {
                    self.lock = Some(qc.clone());
                    self.block = Some(proposal.block.clone());
                } else {
                    return Err(OverlordError::warn_abnormal_lock());
                }
            } else {
                self.lock = Some(qc.clone());
                self.block = Some(proposal.block.clone());
            }
        } else if self.lock.is_none() {
            self.block = Some(proposal.block.clone());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Display, Default, Eq, PartialEq)]
#[display(fmt = "height: {}, round: {}, step: {}", height, round, step)]
pub struct Stage {
    pub height: Height,
    pub round:  Round,
    pub step:   Step,
}

impl Stage {
    pub fn new(height: Height, round: Round, step: Step) -> Self {
        Stage {
            height,
            round,
            step,
        }
    }

    pub fn next_height(&mut self) {
        self.height += 1;
        self.round = INIT_ROUND;
        self.step = Step::Propose;
    }

    // if round jump return true
    pub fn update_stage(&mut self, stage: Stage) -> bool {
        assert!(*self >= stage);
        let is_jump = stage.round > self.round;
        *self = stage;
        is_jump
    }
}

impl PartialOrd for Stage {
    fn partial_cmp(&self, other: &Stage) -> Option<Ordering> {
        Some(
            self.height
                .cmp(&other.height)
                .then(self.round.cmp(&other.round))
                .then(self.step.cmp(&other.step)),
        )
    }
}

impl Ord for Stage {
    fn cmp(&self, other: &Stage) -> Ordering {
        self.height
            .cmp(&other.height)
            .then(self.round.cmp(&other.round))
            .then(self.step.cmp(&other.step))
    }
}

pub trait NextStage {
    fn next_stage(&self) -> Stage;
}

impl<B: Blk> NextStage for SignedProposal<B> {
    fn next_stage(&self) -> Stage {
        Stage::new(self.proposal.height, self.proposal.round, Step::PreVote)
    }
}

impl NextStage for PreVoteQC {
    fn next_stage(&self) -> Stage {
        Stage::new(self.vote.height, self.vote.round, Step::PreCommit)
    }
}

impl NextStage for PreCommitQC {
    fn next_stage(&self) -> Stage {
        Stage::new(self.vote.height, self.vote.round, Step::Commit)
    }
}

impl NextStage for ChokeQC {
    fn next_stage(&self) -> Stage {
        Stage::new(self.choke.height, self.choke.round + 1, Step::Propose)
    }
}

#[derive(Clone, Debug, Display, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    #[display(fmt = "Propose step")]
    Propose,
    #[display(fmt = "PreVote step")]
    PreVote,
    #[display(fmt = "PreCommit step")]
    PreCommit,
    #[display(fmt = "Brake step")]
    Brake,
    #[display(fmt = "Commit step")]
    Commit,
}

impl Default for Step {
    fn default() -> Self {
        Step::Propose
    }
}

impl Into<u8> for Step {
    fn into(self) -> u8 {
        match self {
            Step::Propose => 0,
            Step::PreVote => 1,
            Step::PreCommit => 2,
            Step::Brake => 3,
            Step::Commit => 4,
        }
    }
}

impl From<u8> for Step {
    fn from(s: u8) -> Self {
        match s {
            0 => Step::Propose,
            1 => Step::PreVote,
            2 => Step::PreCommit,
            3 => Step::Brake,
            4 => Step::Commit,
            _ => panic!("Invalid Step type!"),
        }
    }
}

#[derive(Debug, Display)]
#[display(
    fmt = "exec_height: {}, pre_proof: {}, pre_hash: {}",
    exec_height,
    pre_proof,
    "hex::encode(pre_hash.clone())"
)]
pub struct ProposePrepare<S: St> {
    pub exec_height:  Height,
    pub block_states: BTreeMap<Height, S>,

    pub pre_proof: Proof, /* proof for the previous block which will be involved in the
                           * next block */
    pub pre_hash: Hash,
}

impl<S: St> ProposePrepare<S> {
    pub fn new(
        exec_height: Height,
        block_states: Vec<BlockState<S>>,
        pre_proof: Proof,
        pre_hash: Hash,
    ) -> Self {
        let block_states = block_states
            .into_iter()
            .map(|state| (state.height, state.state))
            .collect();
        ProposePrepare {
            exec_height,
            block_states,
            pre_proof,
            pre_hash,
        }
    }

    pub fn get_block_state_vec(&self) -> Vec<BlockState<S>> {
        self.block_states
            .iter()
            .map(|(height, state)| BlockState::new(*height, state.clone()))
            .collect()
    }
}
