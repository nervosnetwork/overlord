use tokio::sync::{mpsc::UnboundedReceiver, watch::Sender};

use crate::error::ConsensusError;
use crate::smr::smr_types::{Lock, SMREvent, SMRTrigger, Step, TriggerSource, TriggerType};
use crate::types::Hash;
use crate::{ConsensusResult, INIT_EPOCH_ID, INIT_ROUND};

/// A smallest implementation of an atomic overlord state machine. It
#[derive(Debug)]
pub struct StateMachine {
    epoch_id:      u64,
    round:         u64,
    step:          Step,
    proposal_hash: Hash,
    lock:          Option<Lock>,

    event:   Sender<SMREvent>,
    trigger: UnboundedReceiver<SMRTrigger>,
}

impl StateMachine {
    pub fn new(
        event_sender: Sender<SMREvent>,
        trigger_receiver: UnboundedReceiver<SMRTrigger>,
    ) -> Self {
        StateMachine {
            epoch_id:      INIT_EPOCH_ID,
            round:         INIT_ROUND,
            step:          Step::default(),
            proposal_hash: Hash::new(),
            lock:          None,
            trigger:       trigger_receiver,
            event:         event_sender,
        }
    }

    pub async fn process_events(&mut self) -> ConsensusResult<()> {
        if let Some(trigger) = self.trigger.recv().await {
            let trigger_type = trigger.trigger_type.clone();
            match trigger_type {
                TriggerType::NewEpoch(epoch_id) => {
                    self.handle_new_epoch(epoch_id, trigger.source)?;
                }
                TriggerType::Proposal => {
                    self.handle_proposal(trigger.hash, trigger.round, trigger.source)?;
                }
                TriggerType::PrevoteQC => {
                    self.handle_prevote(trigger.hash, trigger.round, trigger.source)?;
                }
                TriggerType::PrecommitQC => {
                    self.handle_precommit(trigger.hash, trigger.round, trigger.source)?;
                }
            }
        }
        Ok(())
    }

    /// Handle a new epoch trigger. If new epoch ID is higher than current, goto a new epoch and
    /// throw a new round info event.
    fn handle_new_epoch(&mut self, epoch_id: u64, source: TriggerSource) -> ConsensusResult<()> {
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
            round:         0u64,
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
        _source: TriggerSource,
    ) -> ConsensusResult<()> {
        if self.step != Step::Propose {
            return Ok(());
        }

        self.check()?;
        if proposal_hash.is_empty() && lock_round.is_some() {
            return Err(ConsensusError::ProposalErr("Invalid lock".to_string()));
        }

        // update PoLC
        if let Some(lround) = lock_round {
            if let Some(lock) = self.lock.clone() {
                if lround > lock.round {
                    self.remove_polc();
                    self.set_proposal(proposal_hash.clone());
                } else if lround == lock.round && proposal_hash != self.proposal_hash {
                    return Err(ConsensusError::CorrectnessErr("Fork".to_string()));
                }
            } else {
                self.set_proposal(proposal_hash.clone());
            }
        } else if self.lock.is_none() {
            self.proposal_hash = proposal_hash;
        }

        // throw prevote vote event
        self.throw_event(SMREvent::PrevoteVote(self.proposal_hash.clone()))?;
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
        _source: TriggerSource,
    ) -> ConsensusResult<()> {
        if self.step != Step::Prevote {
            return Ok(());
        }

        self.check()?;
        if prevote_round.is_none() {
            return Err(ConsensusError::PrevoteErr("No vote round".to_string()));
        }

        // update PoLC
        let vote_round = prevote_round.unwrap();
        self.check_round(vote_round)?;
        if let Some(lock) = self.lock.clone() {
            if vote_round > lock.round {
                self.update_polc(prevote_hash.clone(), vote_round);
            }
        } else {
            self.update_polc(prevote_hash.clone(), vote_round);
        }

        // throw precommit vote event
        self.throw_event(SMREvent::PrecommitVote(self.proposal_hash.clone()))?;
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
        _source: TriggerSource,
    ) -> ConsensusResult<()> {
        if self.step != Step::Precommit {
            return Ok(());
        }

        self.check()?;
        if precommit_round.is_none() {
            return Err(ConsensusError::PrevoteErr("No vote round".to_string()));
        }

        self.check_round(precommit_round.unwrap())?;
        if precommit_hash.is_empty() {
            let (lround, lproposal) = if let Some(lock) = self.lock.clone() {
                (Some(lock.round), Some(lock.hash))
            } else {
                (None, None)
            };

            // throw new round info event
            self.throw_event(SMREvent::NewRoundInfo {
                round:         self.round + 1,
                lock_round:    lround,
                lock_proposal: lproposal,
            })?;
            self.goto_next_round();
        } else {
            if let Some(lock) = self.lock.clone() {
                if lock.hash != precommit_hash {
                    return Err(ConsensusError::CorrectnessErr("Fork".to_string()));
                }
            }
            self.update_polc(precommit_hash.clone(), self.round);
            self.throw_event(SMREvent::Commit(precommit_hash))?;
            self.goto_step(Step::Commit);
        }

        Ok(())
    }

    #[inline]
    fn throw_event(&mut self, event: SMREvent) -> ConsensusResult<()> {
        self.event
            .broadcast(event.clone())
            .map_err(|_| ConsensusError::ThrowEventErr(format!("{}", event)))?;
        Ok(())
    }

    /// Goto a new epoch and clear everything.
    #[inline]
    fn goto_new_epoch(&mut self, epoch_id: u64) {
        self.epoch_id = epoch_id;
        self.round = 0;
        self.goto_step(Step::Propose);
        self.proposal_hash = Hash::new();
        self.lock = None;
    }

    /// Keep the lock, if any, when go to the next round.
    #[inline]
    fn goto_next_round(&mut self) {
        self.round += 1;
        self.proposal_hash.clear();
        self.goto_step(Step::Propose);
        if self.lock.is_some() {
            self.proposal_hash = self.lock.clone().unwrap().hash;
        }
    }

    /// Goto the given step.
    #[inline]
    fn goto_step(&mut self, step: Step) {
        self.step = step;
    }

    /// Update the PoLC. Firstly set self proposal as the given hash. Secondly update the PoLC. If
    /// the hash is empty, remove it. Otherwise, set lock round and hash as the given round and
    /// hash.
    #[inline]
    fn update_polc(&mut self, hash: Hash, round: u64) {
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
        self.proposal_hash = proposal_hash;
    }

    /// Check if the given round is equal to self round.
    #[inline]
    fn check_round(&mut self, round: u64) -> ConsensusResult<()> {
        if self.round != round {
            return Err(ConsensusError::RoundDiff {
                local: self.round,
                vote:  round,
            });
        }
        Ok(())
    }

    /// Do below self checks before each message is processed:
    /// 1. Whenever the lock is some and the proposal hash is empty, is impossible.
    /// 2. As long as there is a lock, the lock and proposal hash must be consistent.
    /// 3. Before precommit step, and round is 0, there can be no lock.
    /// 4. If the step is propose, proposal hash must be empty unless lock is some.
    #[inline]
    fn check(&mut self) -> ConsensusResult<()> {
        // Whenever self proposal is empty but self lock is some, is not correct.
        if self.proposal_hash.is_empty() && self.lock.is_some() {
            return Err(ConsensusError::SelfCheckErr(format!(
                "Invalid lock, epoch ID {}, round {}",
                self.epoch_id, self.round
            )));
        }

        // Lock hash must be same as proposal hash, if has.
        if self.lock.is_some() {
            if self.lock.clone().unwrap().hash != self.proposal_hash {
                return Err(ConsensusError::SelfCheckErr("Lock".to_string()));
            }
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
            && !self.proposal_hash.is_empty()
            && self.lock.is_none()
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
