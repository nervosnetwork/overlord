#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]

use std::cmp::{Ord, Ordering, PartialOrd};
use std::collections::BTreeMap;
use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;

use bytes::Bytes;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::UnboundedReceiver;
use log::info;
use rlp::{decode, encode, DecoderError};

use crate::auth::AuthManage;
use crate::cabinet::{Cabinet, Capsule};
use crate::exec::Exec;
use crate::smr::EventAgent;
use crate::timeout::TimeoutEvent;
use crate::types::{
    Choke, ChokeQC, PreCommitQC, PreVoteQC, Proposal, SignedChoke, SignedPreCommit, SignedPreVote,
    SignedProposal, UpdateFrom,
};
use crate::{
    Adapter, Address, Blk, BlockState, CommonHex, ExecResult, Hash, Height, OverlordConfig,
    OverlordError, OverlordMsg, OverlordResult, PriKeyHex, Proof, Round, St, TimeConfig, TinyHex,
    Wal, INIT_ROUND,
};

#[derive(Clone, Debug, Display, Default, Eq, PartialEq)]
#[display(
    fmt = "stage: {}, lock: {}, pre_commit_qc: {}, from: {}",
    stage,
    "lock.clone().map_or(\"None\".to_owned(), |lock| format!(\"{}\", lock))",
    "pre_commit_qc.clone().map_or(\"None\".to_owned(), |qc| format!(\"{}\", qc))",
    "from.clone().map_or(\"None\".to_owned(), |from| format!(\"{}\", from))"
)]
pub struct StateInfo<B: Blk> {
    pub address: Address,
    pub stage:   Stage,

    pub lock:          Option<PreVoteQC>,
    pub block:         Option<B>,
    pub pre_commit_qc: Option<PreCommitQC>,
    pub from:          Option<UpdateFrom>,
}

impl<B: Blk> StateInfo<B> {
    pub fn from_commit_height(commit_height: Height, my_address: Address) -> Self {
        StateInfo {
            address:       my_address,
            stage:         Stage::new(commit_height, INIT_ROUND, Step::Commit),
            lock:          None,
            pre_commit_qc: None,
            block:         None,
            from:          None,
        }
    }

    pub fn handle_signed_proposal(&mut self, sp: &SignedProposal<B>) -> OverlordResult<()> {
        let next_stage = self.filter_stage(sp)?;

        let old_stage = self.clone();
        if self.stage.update_stage(next_stage) {
            self.from = Some(UpdateFrom::PreVoteQC(
                sp.proposal
                    .lock
                    .clone()
                    .expect("Unreachable! Have checked lock exists before"),
            ));
        }
        self.update_lock(&sp.proposal)?;
        self.log_state_update_of_msg(old_stage, sp.into());
        Ok(())
    }

    pub fn handle_pre_vote_qc(&mut self, pre_vote_qc: &PreVoteQC, block: B) -> OverlordResult<()> {
        let next_stage = self.filter_stage(pre_vote_qc)?;

        let old_stage = self.clone();
        if self.stage.update_stage(next_stage) {
            self.from = Some(UpdateFrom::PreVoteQC(pre_vote_qc.clone()));
        }
        if pre_vote_qc.vote.is_empty_vote() {
            self.lock = None;
            self.block = None;
        } else {
            self.lock = Some(pre_vote_qc.clone());
            self.block = Some(block);
        }
        self.log_state_update_of_msg(old_stage, pre_vote_qc.into());
        Ok(())
    }

    pub fn handle_pre_commit_qc(
        &mut self,
        pre_commit_qc: &PreCommitQC,
        block: B,
    ) -> OverlordResult<()> {
        let next_stage = self.filter_stage(pre_commit_qc)?;

        let old_stage = self.clone();
        if self.stage.update_stage(next_stage) {
            self.from = Some(UpdateFrom::PreCommitQC(pre_commit_qc.clone()));
        }
        if !pre_commit_qc.vote.is_empty_vote() {
            self.pre_commit_qc = Some(pre_commit_qc.clone());
            self.block = Some(block);
        }
        self.log_state_update_of_msg(old_stage, pre_commit_qc.into());
        Ok(())
    }

    pub fn handle_choke_qc(&mut self, choke_qc: &ChokeQC) -> OverlordResult<()> {
        let next_stage = self.filter_stage(choke_qc)?;

        let old_stage = self.clone();
        if self.stage.update_stage(next_stage) {
            self.from = Some(UpdateFrom::ChokeQC(choke_qc.clone()));
        }
        self.log_state_update_of_msg(old_stage, choke_qc.into());
        Ok(())
    }

    pub fn handle_timeout(&mut self, stage: &Stage) -> OverlordResult<()> {
        let next_stage = self.filter_stage(stage)?;

        let old_stage = self.clone();
        self.stage.update_stage(next_stage);
        self.log_state_update_of_timeout(old_stage, stage.clone().into());

        Ok(())
    }

    pub fn filter_stage<T: NextStage>(&self, msg: &T) -> OverlordResult<Stage> {
        let next_stage = msg.next_stage();
        if next_stage < self.stage || (next_stage == self.stage && self.stage.step != Step::Brake) {
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

    pub fn next_height(&mut self) {
        self.stage.next_height();
        self.lock = None;
        self.pre_commit_qc = None;
        self.block = None;
        self.from = None;
    }

    fn log_state_update_of_msg(&self, old_state: StateInfo<B>, msg: Capsule<B>) {
        info!(
            "[MESSAGE] \n\t<{}> <=\t{} \n\tbefore : {} \n\tupdated: {}\n",
            self.address.tiny_hex(),
            msg,
            old_state,
            self
        );
    }

    fn log_state_update_of_timeout(&self, old_state: StateInfo<B>, timeout: TimeoutEvent) {
        info!(
            "[TIMEOUT] \n\t<{}> <-\t{} \n\tbefore : {} \n\tupdated: {}\n",
            self.address.tiny_hex(),
            timeout,
            old_state,
            self
        );
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
        info!("goto new height {}", self.height + 1);
        self.height += 1;
        self.round = INIT_ROUND;
        self.step = Step::Propose;
    }

    // if round jump return true
    pub fn update_stage(&mut self, stage: Stage) -> bool {
        // assert!(*self < stage, "self {}, update {}", self, stage);
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

impl NextStage for Stage {
    // timeout flow
    fn next_stage(&self) -> Stage {
        match self.step {
            Step::Propose => Stage::new(self.height, self.round, Step::PreVote),
            Step::PreVote => Stage::new(self.height, self.round, Step::PreCommit),
            Step::PreCommit => Stage::new(self.height, self.round, Step::Brake),
            Step::Brake => Stage::new(self.height, self.round, Step::Brake),
            Step::Commit => Stage::new(self.height + 1, self.round, Step::Propose),
        }
    }
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
        if self.vote.is_empty_vote() {
            Stage::new(self.vote.height, self.vote.round, Step::Brake)
        } else {
            Stage::new(self.vote.height, self.vote.round, Step::Commit)
        }
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
    "pre_hash.tiny_hex()"
)]
pub struct ProposePrepare<S: St> {
    pub exec_height:  Height,
    pub exec_results: BTreeMap<Height, ExecResult<S>>,

    pub pre_proof: Proof, /* proof for the previous block which will be involved in the
                           * next block */
    pub pre_hash: Hash,
}

impl<S: St> ProposePrepare<S> {
    pub fn new(
        exec_height: Height,
        exec_results: Vec<ExecResult<S>>,
        pre_proof: Proof,
        pre_hash: Hash,
    ) -> Self {
        let exec_results = exec_results
            .into_iter()
            .map(|rst| (rst.block_states.height, rst))
            .collect();
        ProposePrepare {
            exec_height,
            exec_results,
            pre_proof,
            pre_hash,
        }
    }

    pub fn handle_exec_result(&mut self, exec_result: ExecResult<S>) {
        let exec_height = exec_result.block_states.height;
        self.exec_height = exec_height;
        self.exec_results.insert(exec_height, exec_result);
    }

    pub fn handle_commit(
        &mut self,
        block_hash: Hash,
        pre_commit_qc: PreCommitQC,
        commit_exec_h: Height,
        next_height: Height,
    ) -> ExecResult<S> {
        self.pre_hash = block_hash;
        self.pre_proof = pre_commit_qc;
        let commit_exec_result = self
            .exec_results
            .get(&commit_exec_h)
            .expect("Unreachable! Cannot get commit exec result when commit")
            .clone();
        self.exec_results = self.exec_results.split_off(&next_height);
        commit_exec_result
    }

    pub fn get_block_states_list(&self, cut_off: Height) -> Vec<BlockState<S>> {
        self.exec_results
            .iter()
            .filter(|(h, _)| **h <= cut_off)
            .map(|(height, exec_result)| exec_result.block_states.clone())
            .collect()
    }
}

#[test]
fn test_stage_cmp() {
    let stage_0 = Stage::new(10, 0, Step::Propose);
    let stage_1 = Stage::new(10, 0, Step::Propose);
    assert_eq!(stage_0, stage_1);
    let stage_2 = Stage::new(10, 0, Step::PreVote);
    assert!(stage_2 > stage_1);
    let stage_3 = Stage::new(10, 1, Step::Propose);
    assert!(stage_3 > stage_2);
    let stage_4 = Stage::new(9, 1, Step::Commit);
    assert!(stage_4 < stage_3);
}
