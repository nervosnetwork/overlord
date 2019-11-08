use std::{cell::Cell, time::Duration};

use crate::smr::smr_types::SMREvent;
use crate::DurationConfig;
use crate::{error::ConsensusError, ConsensusResult};

/// Overlord timer config.
#[derive(Debug, Clone)]
pub struct TimerConfig {
    interval:  Cell<u64>,
    propose:   (u64, u64),
    prevote:   (u64, u64),
    precommit: (u64, u64),
}

impl TimerConfig {
    pub fn new(interval: u64) -> Self {
        TimerConfig {
            interval:  Cell::new(interval),
            propose:   (24, 30),
            prevote:   (10, 30),
            precommit: (5, 30),
        }
    }

    pub fn update(&mut self, config: DurationConfig) {
        self.propose = config.get_propose_config();
        self.prevote = config.get_prevote_config();
        self.precommit = config.get_precommit_config();
    }

    pub fn get_timeout(&self, event: SMREvent) -> ConsensusResult<Duration> {
        match event {
            SMREvent::NewRoundInfo { .. } => Ok(self.get_propose_timeout()),
            SMREvent::PrevoteVote { .. } => Ok(self.get_prevote_timeout()),
            SMREvent::PrecommitVote { .. } => Ok(self.get_precommit_timeout()),
            _ => Err(ConsensusError::TimerErr("No commit timer".to_string())),
        }
    }

    fn get_propose_timeout(&self) -> Duration {
        Duration::from_millis(self.interval.get() * self.propose.0 / self.propose.1)
    }

    fn get_prevote_timeout(&self) -> Duration {
        Duration::from_millis(self.interval.get() * self.prevote.0 / self.prevote.1)
    }

    fn get_precommit_timeout(&self) -> Duration {
        Duration::from_millis(self.interval.get() * self.precommit.0 / self.precommit.1)
    }
}
