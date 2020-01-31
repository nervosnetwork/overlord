use crate::smr::smr_types::{Lock, Step};
use crate::types::{AggregatedVote, PoLC};
use crate::Codec;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WalInfo<T: Codec> {
    pub height: u64,
    pub round:  u64,
    pub step:   Step,
    pub lock:   Option<WalLock<T>>,
}

impl<T: Codec> WalInfo<T> {
    pub fn to_smr_base(&self) -> SMRBase {
        let lock = if let Some(polc) = &self.lock {
            Some(polc.to_lock())
        } else {
            None
        };

        SMRBase {
            height: self.height,
            round:  self.round,
            step:   self.step.clone(),
            polc:   lock,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalLock<T: Codec> {
    pub lock_round: u64,
    pub lock_votes: AggregatedVote,
    pub content:    T,
}

impl<T: Codec> WalLock<T> {
    pub fn to_polc(&self) -> PoLC {
        PoLC {
            lock_round: self.lock_round,
            lock_votes: self.lock_votes.clone(),
        }
    }

    pub fn to_lock(&self) -> Lock {
        Lock {
            round: self.lock_round,
            hash:  self.lock_votes.epoch_hash.clone(),
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
