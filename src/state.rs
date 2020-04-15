#![allow(unused_imports)]

use std::cmp::{Ord, Ordering, PartialOrd};
use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::UnboundedReceiver;
use serde::{Deserialize, Serialize};

use crate::auth::AuthManage;
use crate::cabinet::Cabinet;
use crate::types::{Proposal, UpdateFrom};
use crate::{Adapter, Address, Blk, CommonHex, Height, OverlordMsg, PriKeyHex, Round, St, Wal};

#[derive(Serialize, Deserialize, Clone, Debug, Display, Default, Eq, PartialEq)]
#[display(
    fmt = "stage: {}, lock_round: {}, from: {}",
    stage,
    "lock_round.clone().map_or(\"None\".to_owned(), |lock_round| format!(\"{}\", lock_round))",
    "from.clone().map_or(\"None\".to_owned(), |from| format!(\"{}\", from))"
)]
pub struct StateInfo {
    pub stage:      Stage,
    pub lock_round: Option<Round>,
    pub from:       Option<UpdateFrom>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Display, Default, Eq, PartialEq)]
#[display(fmt = "height: {}, round: {}, step: {}", height, round, step)]
pub struct Stage {
    pub height: Height,
    pub round:  Round,
    pub step:   Step,
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

#[derive(Clone, Debug, Display, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
