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
    Adapter, Address, Blk, CommonHex, Height, OverlordConfig, OverlordMsg, PriKeyHex, Proof, Round,
    St, TimeConfig, Wal, INIT_ROUND,
};

#[derive(Clone, Debug, Display, Default, Eq, PartialEq)]
#[display(
    fmt = "stage: {}, polc: {}, pre_commit_qc: {}, from: {}, pre_proof: {}",
    stage,
    "polc.clone().map_or(\"None\".to_owned(), |polc| format!(\"{}\", polc))",
    "pre_commit_qc.clone().map_or(\"None\".to_owned(), |qc| format!(\"{}\", qc))",
    "from.clone().map_or(\"None\".to_owned(), |from| format!(\"{}\", from))",
    pre_proof
)]
pub struct StateInfo<B: Blk> {
    pub stage:         Stage,
    pub time_config:   TimeConfig,
    pub polc:          Option<PoLC>,
    pub pre_commit_qc: Option<PreCommitQC>,
    pub block:         Option<B>,
    pub from:          Option<UpdateFrom>,
    pub pre_proof:     Proof,
}

impl<B: Blk> StateInfo<B> {
    pub fn new(height: Height, time_config: TimeConfig, pre_proof: Proof) -> Self {
        StateInfo {
            stage: Stage::new(height),
            time_config,
            pre_proof,
            polc: None,
            pre_commit_qc: None,
            block: None,
            from: None,
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
pub enum StateError {
    #[display(fmt = "{}", _0)]
    Wal(WalError),
    #[display(fmt = "{:?}", _0)]
    Decode(DecoderError),
}

impl Error for StateError {}
