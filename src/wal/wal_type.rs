use derive_more::Display;

use crate::smr::smr_types::{Lock, Step};
use crate::types::{AggregatedVote, UpdateFrom};
use crate::Codec;

#[derive(Clone, Debug, Display, Eq, PartialEq)]
#[rustfmt::skip]
#[display(
    fmt = "wal info height {}, round {}, step {:?}",
    height, round, step,
)]
/// Structure of Wal Info
pub struct WalInfo<T: Codec> {
    /// height
    pub height: u64,
    /// round
    pub round:  u64,
    /// step
    pub step:   Step,
    /// lock
    pub lock:   Option<WalLock<T>>,
    /// from
    pub from:   UpdateFrom,
}

impl<T: Codec> WalInfo<T> {
    /// transfer WalInfo to SMRBase
    pub fn into_smr_base(self) -> SMRBase {
        SMRBase {
            height: self.height,
            round:  self.round,
            step:   self.step.clone(),
            polc:   self.lock.map(|polc| polc.to_lock()),
        }
    }
}

#[derive(Clone, Debug, Display, PartialEq, Eq)]
#[display(fmt = "wal lock round {}, qc {:?}", lock_round, lock_votes)]
pub struct WalLock<T: Codec> {
    pub lock_round: u64,
    pub lock_votes: AggregatedVote,
    pub content:    T,
}

impl<T: Codec> WalLock<T> {
    pub fn to_lock(&self) -> Lock {
        Lock {
            round: self.lock_round,
            hash:  self.lock_votes.block_hash.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SMRBase {
    pub height: u64,
    pub round:  u64,
    pub step:   Step,
    pub polc:   Option<Lock>,
}