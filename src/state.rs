#![allow(unused_imports)]

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
use crate::types::{PoLC, PreCommitQC, Proposal, UpdateFrom};
use crate::wal::WalError;
use crate::{
    Adapter, Address, Blk, BlockState, CommonHex, Hash, Height, OverlordConfig, OverlordMsg,
    PriKeyHex, Proof, Round, St, TimeConfig, Wal, INIT_ROUND,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Display, Default, Eq, PartialEq)]
#[display(
    fmt = "stage: {}, polc: {}, pre_commit_qc: {}, from: {}",
    stage,
    "polc.clone().map_or(\"None\".to_owned(), |polc| format!(\"{}\", polc))",
    "pre_commit_qc.clone().map_or(\"None\".to_owned(), |qc| format!(\"{}\", qc))",
    "from.clone().map_or(\"None\".to_owned(), |from| format!(\"{}\", from))"
)]
pub struct StateInfo<B: Blk> {
    // current info
    pub stage: Stage,

    pub polc:          Option<PoLC>,
    pub pre_commit_qc: Option<PreCommitQC>,
    pub block:         Option<B>,
    pub from:          Option<UpdateFrom>,
}

impl<B: Blk> StateInfo<B> {
    pub fn new(height: Height) -> Self {
        StateInfo {
            stage:         Stage::new(height),
            polc:          None,
            pre_commit_qc: None,
            block:         None,
            from:          None,
        }
    }

    pub fn from_wal(wal: &Wal) -> Result<Self, StateError> {
        let encode = wal.load_state().map_err(StateError::Wal)?;
        decode(&encode).map_err(StateError::Decode)
    }

    pub fn save_wal(&self, wal: &Wal) -> Result<(), StateError> {
        let encode = encode(self);
        wal.save_state(&Bytes::from(encode))
            .map_err(StateError::Wal)
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
    pub fn new(height: Height) -> Self {
        Stage {
            height,
            round: INIT_ROUND,
            step: Step::Propose,
        }
    }

    pub fn next_height(&mut self) {
        self.height += 1;
        self.round = INIT_ROUND;
        self.step = Step::Propose;
    }

    pub fn next_round(&mut self) {
        self.round += 1;
        self.step = Step::Propose;
    }

    pub fn goto_step(&mut self, step: Step) {
        assert!(self.step >= step);
        self.step = step;
    }

    pub fn update_stage(&mut self, stage: Stage) {
        assert!(*self >= stage);
        *self = stage;
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
    fmt = "time_config: {}, exec_height: {}, pre_proof: {}, pre_hash: {}",
    time_config,
    exec_height,
    pre_proof,
    "hex::encode(pre_hash.clone())"
)]
pub struct ProposePrepare<S: St> {
    pub time_config: TimeConfig,

    pub exec_height:  Height,
    pub block_states: BTreeMap<Height, S>,

    pub pre_proof: Proof, /* proof for the previous block which will be involved in the
                           * next block */
    pub pre_hash: Hash,
}

impl<S: St> ProposePrepare<S> {
    pub fn new(
        time_config: TimeConfig,
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
            time_config,
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

#[derive(Debug, Display)]
pub enum StateError {
    #[display(fmt = "{}", _0)]
    Wal(WalError),
    #[display(fmt = "{:?}", _0)]
    Decode(DecoderError),
}

impl Error for StateError {}
