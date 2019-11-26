use std::ops::BitXor;
use std::pin::Pin;
use std::task::{Context, Poll};

use derive_more::Display;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::stream::Stream;
use log::{debug, info};

use crate::smr::smr_types::{Lock, SMREvent, SMRTrigger, Step, TriggerSource, TriggerType};
use crate::{error::ConsensusError, smr::Event, types::Hash};
use crate::{ConsensusResult, INIT_EPOCH_ID, INIT_ROUND};

/// A smallest implementation of an atomic overlord state machine. It
#[derive(Debug, Display)]
#[rustfmt::skip]
#[display(fmt = "State machine epoch ID {}, round {}, step {:?}", epoch_id, round, step)]
pub struct StateMachine {
    epoch_id:      u64,
    round:         u64,
    step:          Step,
    epoch_hash: Hash,
    lock:          Option<Lock>,

    event:   (UnboundedSender<SMREvent>, UnboundedSender<SMREvent>),
    trigger: UnboundedReceiver<SMRTrigger>,
}

impl Stream for StateMachine {
    type Item = ConsensusError;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match Stream::poll_next(Pin::new(&mut self.trigger), cx) {
            Poll::Pending => Poll::Pending,

            Poll::Ready(msg) => {
                if msg.is_none() {
                    return Poll::Ready(Some(ConsensusError::TriggerSMRErr(
                        "Channel dropped".to_string(),
                    )));
                }

                let msg = msg.unwrap();
                let trigger_type = msg.trigger_type.clone();
                let res = match trigger_type {
                    TriggerType::NewEpoch(epoch_id) => self.handle_new_epoch(epoch_id, msg.source),
                    TriggerType::Proposal => {
                        self.handle_proposal(msg.hash, msg.round, msg.source, msg.epoch_id)
                    }
                    TriggerType::PrevoteQC => {
                        self.handle_prevote(msg.hash, msg.round, msg.source, msg.epoch_id)
                    }
                    TriggerType::PrecommitQC => {
                        self.handle_precommit(msg.hash, msg.round, msg.source, msg.epoch_id)
                    }
                };

                if res.is_err() {
                    Poll::Ready(Some(res.err().unwrap()))
                } else {
                    Poll::Ready(None)
                }
            }
        }
    }
}

impl StateMachine {
    /// Create a new state machine.
    pub fn new(trigger_receiver: UnboundedReceiver<SMRTrigger>) -> (Self, Event, Event) {
        let (tx_1, rx_1) = unbounded();
        let (tx_2, rx_2) = unbounded();

        let state_machine = StateMachine {
            epoch_id:   INIT_EPOCH_ID,
            round:      INIT_ROUND,
            step:       Step::default(),
            epoch_hash: Hash::new(),
            lock:       None,
            trigger:    trigger_receiver,
            event:      (tx_1, tx_2),
        };

        (state_machine, Event::new(rx_1), Event::new(rx_2))
    }

    /// Handle a new epoch trigger. If new epoch ID is higher than current, goto a new epoch and
    /// throw a new round info event.
    fn handle_new_epoch(&mut self, epoch_id: u64, source: TriggerSource) -> ConsensusResult<()> {
        info!("Overlord: SMR triggered by new epoch {}", epoch_id);

        if source != TriggerSource::State {
            return Err(ConsensusError::Other(
                "Rich status source error".to_string(),
            ));
        } else if epoch_id <= self.epoch_id {
            return Err(ConsensusError::Other("Delayed status".to_string()));
        }

        self.check()?;
        self.goto_new_epoch(epoch_id);

        // throw new round info event
        self.throw_event(SMREvent::NewRoundInfo {
            epoch_id:      self.epoch_id,
            round:         INIT_ROUND,
            lock_round:    None,
            lock_proposal: None,
        })?;
        Ok(())
    }

    /// Handle a proposal trigger. Only if self step is propose, the proposal is valid.
    /// If proposal hash is empty, prevote to an empty hash. If the lock round is some, and the lock
    /// round is higher than self lock round, remove PoLC. Fianlly throw prevote vote event. It is
    /// impossible that the proposal hash is empty with the lock round is some.
    fn handle_proposal(
        &mut self,
        proposal_hash: Hash,
        lock_round: Option<u64>,
        source: TriggerSource,
        epoch_id: u64,
    ) -> ConsensusResult<()> {
        if self.epoch_id != epoch_id {
            return Ok(());
        }

        if self.step > Step::Propose {
            return Ok(());
        }

        info!(
            "Overlord: SMR triggered by a proposal hash {:?}, from {:?}",
            proposal_hash, source
        );

        // If the proposal trigger is from timer, goto prevote step directly.
        if source == TriggerSource::Timer {
            // This event is for timer to set a prevote timer.
            self.throw_event(SMREvent::PrevoteVote {
                epoch_id:   self.epoch_id,
                round:      self.round,
                epoch_hash: Hash::new(),
            })?;
            self.goto_step(Step::Prevote);
            return Ok(());
        } else if proposal_hash.is_empty() {
            return Err(ConsensusError::ProposalErr("Empty proposal".to_string()));
        }

        // update PoLC
        self.check()?;
        if let Some(lock_round) = lock_round {
            if let Some(lock) = self.lock.clone() {
                debug!("Overlord: SMR handle proposal with a lock");

                if lock_round > lock.round {
                    self.remove_polc();
                    self.set_proposal(proposal_hash);
                } else if lock_round == lock.round && proposal_hash != self.epoch_hash {
                    return Err(ConsensusError::CorrectnessErr("Fork".to_string()));
                }
            } else {
                self.set_proposal(proposal_hash);
            }
        } else if self.lock.is_none() {
            self.set_proposal(proposal_hash);
        }

        // throw prevote vote event
        self.throw_event(SMREvent::PrevoteVote {
            epoch_id:   self.epoch_id,
            round:      self.round,
            epoch_hash: self.epoch_hash.clone(),
        })?;
        self.goto_step(Step::Prevote);
        Ok(())
    }

    /// Handle a prevote quorum certificate trigger. Only if self step is prevote, the prevote QC is
    /// valid.  
    /// The prevote round must be some. If the vote round is higher than self lock round, update
    /// PoLC. Fianlly throw precommit vote event.
    fn handle_prevote(
        &mut self,
        prevote_hash: Hash,
        prevote_round: Option<u64>,
        source: TriggerSource,
        epoch_id: u64,
    ) -> ConsensusResult<()> {
        let prevote_round =
            prevote_round.ok_or_else(|| ConsensusError::PrevoteErr("No vote round".to_string()))?;

        if self.epoch_id != epoch_id {
            return Ok(());
        }

        if self.step > Step::Prevote {
            return Ok(());
        }

        info!(
            "Overlord: SMR triggered by prevote QC hash {:?} from {:?}",
            prevote_hash, source
        );

        if source == TriggerSource::Timer {
            // This event is for timer to set a precommit timer.
            self.throw_event(SMREvent::PrecommitVote {
                epoch_id:   self.epoch_id,
                round:      self.round,
                epoch_hash: Hash::new(),
            })?;
            self.goto_step(Step::Precommit);
            return Ok(());
        } else if prevote_hash.is_empty() {
            return Err(ConsensusError::PrevoteErr("Empty qc".to_string()));
        }

        // A prevote QC from timer which means prevote timeout can not lead to unlock. Therefore,
        // only prevote QCs from state will update the PoLC. If the prevote QC is from timer, goto
        // precommit step directly.
        self.check()?;
        let vote_round = prevote_round;
        if let Some(lock) = self.lock.clone() {
            if vote_round > lock.round {
                self.update_polc(prevote_hash, vote_round);
            }
        } else {
            self.update_polc(prevote_hash, vote_round);
        }

        if self.round > vote_round {
            self.round = vote_round;
        }

        // throw precommit vote event
        self.throw_event(SMREvent::PrecommitVote {
            epoch_id:   self.epoch_id,
            round:      self.round,
            epoch_hash: self.epoch_hash.clone(),
        })?;
        self.goto_step(Step::Precommit);
        Ok(())
    }

    /// Handle a precommit quorum certificate trigger. Only if self step is precommit, the precommit
    /// QC is valid.
    /// The precommit round must be some. If its hash is empty, throw new round event and goto next
    /// round. Otherwise, throw commit event.
    fn handle_precommit(
        &mut self,
        precommit_hash: Hash,
        precommit_round: Option<u64>,
        source: TriggerSource,
        epoch_id: u64,
    ) -> ConsensusResult<()> {
        let precommit_round = precommit_round
            .ok_or_else(|| ConsensusError::PrevoteErr("No vote round".to_string()))?;

        if self.epoch_id != epoch_id {
            return Ok(());
        }

        info!(
            "Overlord: SMR triggered by precommit QC hash {:?}, from {:?}",
            precommit_hash, source
        );

        let (lock_round, lock_proposal) = self
            .lock
            .clone()
            .map_or_else(|| (None, None), |lock| (Some(lock.round), Some(lock.hash)));

        if source == TriggerSource::Timer {
            self.throw_event(SMREvent::NewRoundInfo {
                epoch_id: self.epoch_id,
                round: self.round + 1,
                lock_round,
                lock_proposal,
            })?;
            self.goto_next_round();
            return Ok(());
        } else if precommit_hash.is_empty() {
            return Err(ConsensusError::PrecommitErr("Empty qc".to_string()));
        }

        self.check()?;
        self.check_polc(precommit_hash.clone(), precommit_round)?;
        self.throw_event(SMREvent::Commit(precommit_hash))?;
        self.goto_step(Step::Commit);
        Ok(())
    }

    fn throw_event(&mut self, event: SMREvent) -> ConsensusResult<()> {
        info!("Overlord: SMR throw {:?} event", event);
        self.event
            .0
            .unbounded_send(event.clone())
            .map_err(|_| ConsensusError::ThrowEventErr(format!("{}", event.clone())))?;
        self.event
            .1
            .unbounded_send(event.clone())
            .map_err(|_| ConsensusError::ThrowEventErr(format!("{}", event)))?;
        Ok(())
    }

    // Check PoLC when triggered precommit QC by state. If the epoch hash of the QC is equal to self
    // lock, change self round and do commit, otherwise, it may be fork.
    fn check_polc(&mut self, hash: Hash, round: u64) -> ConsensusResult<()> {
        if let Some(lock) = self.lock.as_mut() {
            if lock.hash != hash {
                return Err(ConsensusError::CorrectnessErr("Fork".to_string()));
            } else {
                lock.round = round;
            }
        } else {
            self.lock = Some(Lock { hash, round });
        }

        self.round = round;
        Ok(())
    }

    /// Goto a new epoch and clear everything.
    fn goto_new_epoch(&mut self, epoch_id: u64) {
        info!("Overlord: SMR goto new epoch: {}", epoch_id);
        self.epoch_id = epoch_id;
        self.round = INIT_ROUND;
        self.goto_step(Step::Propose);
        self.epoch_hash = Hash::new();
        self.lock = None;
    }

    /// Keep the lock, if any, when go to the next round.
    fn goto_next_round(&mut self) {
        info!("Overlord: SMR goto next round {}", self.round + 1);
        self.round += 1;
        self.epoch_hash = Hash::new();
        self.goto_step(Step::Propose);
    }

    /// Goto the given step.
    #[inline]
    fn goto_step(&mut self, step: Step) {
        debug!("Overlord: SMR goto step {:?}", step);
        self.step = step;
    }

    /// Update the PoLC. Firstly set self proposal as the given hash. Secondly update the PoLC. If
    /// the hash is empty, remove it. Otherwise, set lock round and hash as the given round and
    /// hash.
    fn update_polc(&mut self, hash: Hash, round: u64) {
        debug!("Overlord: SMR update PoLC at round {}", round);
        self.set_proposal(hash.clone());

        if hash.is_empty() {
            self.remove_polc();
        } else {
            self.lock = Some(Lock { round, hash });
        }
    }

    #[inline]
    fn remove_polc(&mut self) {
        self.lock = None;
    }

    /// Set self proposal hash as the given hash.
    #[inline]
    fn set_proposal(&mut self, proposal_hash: Hash) {
        self.epoch_hash = proposal_hash;
    }

    /// Do below self checks before each message is processed:
    /// 1. Whenever the lock is some and the proposal hash is empty, is impossible.
    /// 2. As long as there is a lock, the lock and proposal hash must be consistent.
    /// 3. Before precommit step, and round is 0, there can be no lock.
    /// 4. If the step is propose, proposal hash must be empty unless lock is some.
    #[inline(always)]
    fn check(&mut self) -> ConsensusResult<()> {
        debug!("Overlord: SMR do self check");

        // Lock hash must be same as proposal hash, if has.
        if self.epoch_id == 0
            && self.lock.is_some()
            && self.lock.clone().unwrap().hash != self.epoch_hash
        {
            return Err(ConsensusError::SelfCheckErr("Lock".to_string()));
        }

        // While self step lt precommit and round is 0, self lock must be none.
        if self.step < Step::Precommit && self.round == 0 && self.lock.is_some() {
            return Err(ConsensusError::SelfCheckErr(format!(
                "Invalid lock, epoch ID {}, round {}",
                self.epoch_id, self.round
            )));
        }

        // While in propose step, if self lock is none, self proposal must be empty.
        if (self.step == Step::Propose || self.step == Step::Precommit)
            && (self.epoch_hash.is_empty().bitxor(self.lock.is_none()))
        {
            return Err(ConsensusError::SelfCheckErr(format!(
                "Invalid proposal hash, epoch ID {}, round {}",
                self.epoch_id, self.round
            )));
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn set_status(&mut self, round: u64, step: Step, proposal_hash: Hash, lock: Option<Lock>) {
        self.round = round;
        self.goto_step(step);
        self.set_proposal(proposal_hash);
        self.lock = lock;
    }

    #[cfg(test)]
    pub fn get_lock(&mut self) -> Option<Lock> {
        self.lock.clone()
    }
}

#[cfg(test)]
mod test {
    use bytes::Bytes;
    use std::ops::BitXor;

    #[test]
    fn test_xor() {
        let left = Bytes::new();
        let right: Option<u64> = None;
        assert_eq!(left.is_empty().bitxor(&right.is_none()), false);
    }
}
