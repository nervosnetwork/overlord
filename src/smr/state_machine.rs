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
        } else if let Some(lock) = self.lock.clone() {
            if lock.hash == precommit_hash {
                self.throw_event(SMREvent::Commit(precommit_hash))?;
                self.goto_step(Step::Commit);
            }
        } else {
            return Err(ConsensusError::PrecommitErr("No lock".to_string()));
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
    /// 2. If the step is propose, proposal hash must be empty unless lock is some.
    #[inline]
    fn check(&mut self) -> ConsensusResult<()> {
        // It is
        if self.proposal_hash.is_empty() && self.lock.is_some() {
            return Err(ConsensusError::SelfCheckErr(format!(
                "Invalid lock, epoch ID {}, round {}",
                self.epoch_id, self.round
            )));
        }

        if self.step == Step::Propose && !self.proposal_hash.is_empty() && self.lock.is_none() {
            return Err(ConsensusError::SelfCheckErr(format!(
                "Invalid proposal hash, epoch ID {}, round {}",
                self.epoch_id, self.round
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use rand::random;
    use tokio::runtime::Runtime;
    use tokio::sync::{mpsc::unbounded_channel, watch::channel};

    use crate::smr::smr_types::{Lock, SMREvent, SMRTrigger, Step, TriggerSource, TriggerType};
    use crate::smr::state_machine::StateMachine;
    use crate::{error::ConsensusError, types::Hash};

    struct StateMachineTestCase {
        base:        InnerState,
        input:       SMRTrigger,
        output:      SMREvent,
        err:         Option<ConsensusError>,
        should_lock: Option<(u64, Hash)>,
    }

    impl StateMachineTestCase {
        fn new(
            base: InnerState,
            input: SMRTrigger,
            output: SMREvent,
            err: Option<ConsensusError>,
            should_lock: Option<(u64, Hash)>,
        ) -> Self {
            StateMachineTestCase {
                base,
                input,
                output,
                err,
                should_lock,
            }
        }
    }

    struct InnerState {
        round:         u64,
        step:          Step,
        proposal_hash: Hash,
        lock:          Option<Lock>,
    }

    impl InnerState {
        fn new(round: u64, step: Step, proposal_hash: Hash, lock: Option<Lock>) -> Self {
            InnerState {
                round,
                step,
                proposal_hash,
                lock,
            }
        }
    }

    impl SMRTrigger {
        fn new(proposal_hash: Hash, t_type: TriggerType, lock_round: Option<u64>) -> Self {
            SMRTrigger {
                trigger_type: t_type,
                source:       TriggerSource::State,
                hash:         proposal_hash,
                round:        lock_round,
            }
        }
    }

    impl Lock {
        fn new(round: u64, hash: Hash) -> Self {
            Lock { round, hash }
        }
    }

    impl StateMachine {
        fn set_status(&mut self, step: Step, proposal_hash: Hash, lock: Option<Lock>) {
            self.goto_step(step);
            self.set_proposal(proposal_hash);
            self.lock = lock;
        }

        fn get_lock(&mut self) -> Option<Lock> {
            self.lock.clone()
        }
    }

    fn gen_hash() -> Hash {
        Hash::from((0..16).map(|_| random::<u8>()).collect::<Vec<_>>())
    }

    fn trigger_test(
        base: InnerState,
        input: SMRTrigger,
        output: SMREvent,
        err: Option<ConsensusError>,
        should_lock: Option<(u64, Hash)>,
    ) {
        let (mut trigger_tx, trigger_rx) = unbounded_channel::<SMRTrigger>();
        let (event_tx, mut event_rx) = channel::<SMREvent>(SMREvent::Commit(Hash::new()));

        let mut state_machine = StateMachine::new(event_tx, trigger_rx);
        state_machine.set_status(base.step, base.proposal_hash, base.lock);
        trigger_tx.try_send(input).unwrap();

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let res = state_machine.process_events().await;
            if res.is_err() {
                assert_eq!(Err(err.unwrap()), res);
                return;
            }

            if should_lock.is_some() {
                let self_lock = state_machine.get_lock().unwrap();
                let should_lock = should_lock.unwrap();
                assert_eq!(self_lock.round, should_lock.0);
                assert_eq!(self_lock.hash, should_lock.1);
            } else {
                assert!(state_machine.get_lock().is_none());
            }

            loop {
                match event_rx.recv().await {
                    Some(event) => {
                        assert_eq!(output, event);
                        return;
                    }
                    None => continue,
                }
            }
        })
    }

    /// Test state machine handle proposal trigger.
    /// There are a total of *4 × 4 + 3 = 19* test cases.
    #[test]
    fn test_proposal_trigger() {
        let mut index = 1;
        let mut test_cases: Vec<StateMachineTestCase> = Vec::new();

        // Test case 01:
        //      self proposal is empty and self is not lock,
        //      proposal is not nil and no lock.
        // The output should be prevote vote to the proposal hash.
        let hash = gen_hash();
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(0, Step::Propose, Hash::new(), None),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, None),
            SMREvent::PrevoteVote(hash),
            None,
            None,
        ));

        // Test case 02:
        //      self proposal is empty and self is not lock,
        //      proposal is nil and no lock.
        // The output should be prevote vote to the proposal hash which is nil.
        let hash = Hash::new();
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(0, Step::Propose, Hash::new(), None),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, None),
            SMREvent::PrevoteVote(hash),
            None,
            None,
        ));

        // Test case 03:
        //      self proposal is empty and self is not lock,
        //      proposal is nil but is with a lock.
        // This is an incorrect situation, the process will return proposal err.
        let hash = Hash::new();
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, Hash::new(), None),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(0)),
            SMREvent::PrevoteVote(hash),
            Some(ConsensusError::ProposalErr("Invalid lock".to_string())),
            None,
        ));

        // Test case 04:
        //      self proposal is empty and self is not lock,
        //      proposal is not nil and with a lock.
        // The output should be prevote vote to the proposal hash.
        let hash = gen_hash();
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, Hash::new(), None),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(0)),
            SMREvent::PrevoteVote(hash),
            None,
            None,
        ));

        // Test case 05:
        //      self proposal is not empty and self is not lock,
        //      proposal is not nil and with a lock.
        // This is an incorrect situation, the process cannot pass self check.
        let hash = gen_hash();
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, hash.clone(), None),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(0)),
            SMREvent::PrevoteVote(hash),
            Some(ConsensusError::SelfCheckErr("".to_string())),
            None,
        ));

        // Test case 06:
        //      self proposal is not empty and self is not lock,
        //      proposal is not nil and no lock.
        // This is an incorrect situation, the process cannot pass self check.
        let hash = gen_hash();
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, hash.clone(), None),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, None),
            SMREvent::PrevoteVote(hash),
            Some(ConsensusError::SelfCheckErr("".to_string())),
            None,
        ));

        // Test case 07:
        //      self proposal is not empty and self is not lock,
        //      proposal is nil and with a lock.
        // This is an incorrect situation, the process cannot pass self check.
        let hash = gen_hash();
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, hash.clone(), None),
            SMRTrigger::new(Hash::new(), TriggerType::Proposal, Some(0)),
            SMREvent::PrevoteVote(hash),
            Some(ConsensusError::SelfCheckErr("".to_string())),
            None,
        ));

        // Test case 08:
        //      self proposal is not empty and self is not lock,
        //      proposal is nil and no lock.
        // This is an incorrect situation, the process cannot pass self check.
        let hash = gen_hash();
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, hash.clone(), None),
            SMRTrigger::new(Hash::new(), TriggerType::Proposal, None),
            SMREvent::PrevoteVote(hash),
            Some(ConsensusError::SelfCheckErr("".to_string())),
            None,
        ));

        // Test case 09:
        //      self proposal is empty and self is lock,
        //      proposal is nil and no lock.
        // This is an incorrect situation, the process cannot pass self check.
        let hash = Hash::new();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 0,
            hash:  lock_hash,
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, hash.clone(), Some(lock)),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, None),
            SMREvent::PrevoteVote(hash),
            Some(ConsensusError::SelfCheckErr("".to_string())),
            None,
        ));

        // Test case 10:
        //      self proposal is empty and self is lock,
        //      proposal is nil and with a lock.
        // This is an incorrect situation, the process cannot pass self check.
        let hash = Hash::new();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 0,
            hash:  lock_hash,
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, hash.clone(), Some(lock)),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(0)),
            SMREvent::PrevoteVote(hash),
            Some(ConsensusError::SelfCheckErr("".to_string())),
            None,
        ));

        // Test case 11:
        //      self proposal is empty and self is lock,
        //      proposal is not nil and no lock.
        // This is an incorrect situation, the process cannot pass self check.
        let hash = Hash::new();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 0,
            hash:  lock_hash,
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, hash.clone(), Some(lock)),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, None),
            SMREvent::PrevoteVote(hash),
            Some(ConsensusError::SelfCheckErr("".to_string())),
            None,
        ));

        // Test case 12:
        //      self proposal is empty and self is lock,
        //      proposal is not nil and with a lock.
        // This is an incorrect situation, the process cannot pass self check.
        let hash = Hash::new();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 0,
            hash:  lock_hash.clone(),
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, hash, Some(lock)),
            SMRTrigger::new(lock_hash.clone(), TriggerType::Proposal, None),
            SMREvent::PrevoteVote(lock_hash),
            Some(ConsensusError::SelfCheckErr("".to_string())),
            None,
        ));

        // Test case 13:
        //      self proposal is not empty and self is lock,
        //      proposal is nil and no lock.
        // The output should be prevote vote to the self lock hash.
        let hash = Hash::new();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 0,
            hash:  lock_hash.clone(),
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, lock_hash.clone(), Some(lock)),
            SMRTrigger::new(hash, TriggerType::Proposal, None),
            SMREvent::PrevoteVote(lock_hash.clone()),
            None,
            Some((0, lock_hash)),
        ));

        // Test case 14:
        //      self proposal is not empty and self is lock,
        //      proposal is not nil and no lock.
        // The output should be prevote vote to the self lock hash.
        let hash = gen_hash();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 0,
            hash:  lock_hash.clone(),
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, lock_hash.clone(), Some(lock)),
            SMRTrigger::new(hash, TriggerType::Proposal, None),
            SMREvent::PrevoteVote(lock_hash.clone()),
            None,
            Some((0, lock_hash)),
        ));

        // Test case 15:
        //      self proposal is not empty and self is lock,
        //      proposal is nil but with a lock.
        // This is an incorrect situation, the process will return proposal err.
        let hash = Hash::new();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 0,
            hash:  lock_hash.clone(),
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(1, Step::Propose, lock_hash.clone(), Some(lock)),
            SMRTrigger::new(hash, TriggerType::Proposal, Some(0)),
            SMREvent::PrevoteVote(lock_hash.clone()),
            Some(ConsensusError::ProposalErr("Invalid lock".to_string())),
            Some((0, lock_hash)),
        ));

        // Test case 16:
        //      self proposal is not empty and self is lock,
        //      proposal is not nil and with a lock.
        //      proposal lock round lt self lock round.
        // The output should be prevote vote to the proposal hash which is lock.
        let hash = gen_hash();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 1,
            hash:  lock_hash.clone(),
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(2, Step::Propose, lock_hash.clone(), Some(lock)),
            SMRTrigger::new(hash, TriggerType::Proposal, Some(0)),
            SMREvent::PrevoteVote(lock_hash.clone()),
            None,
            Some((1, lock_hash)),
        ));

        // Test case 17:
        //      self proposal is not empty and self is lock,
        //      proposal is not nil and with a lock.
        //      proposal lock round gt self lock round.
        // The output should be prevote vote to the proposal hash which is lock.
        let hash = gen_hash();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 1,
            hash:  lock_hash.clone(),
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(3, Step::Propose, lock_hash.clone(), Some(lock)),
            SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(2)),
            SMREvent::PrevoteVote(hash),
            None,
            None,
        ));

        // Test case 18:
        //      self proposal is not empty and self is lock,
        //      proposal is not nil and with a lock.
        //      proposal lock round and proposal is equal to self lock round and proposal.
        // The output should be prevote vote to the self lock hash.
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 1,
            hash:  lock_hash.clone(),
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(2, Step::Propose, lock_hash.clone(), Some(lock)),
            SMRTrigger::new(lock_hash.clone(), TriggerType::Proposal, Some(1)),
            SMREvent::PrevoteVote(lock_hash.clone()),
            None,
            Some((1, lock_hash)),
        ));

        // Test case 16:
        //      self proposal is not empty and self is lock,
        //      proposal is not nil and with a lock.
        //      proposal lock round is equal to self lock round. However, proposal hash is ne self
        //      lock hash.
        // This is extremely dangerous because it can lead to fork. The process will return
        // correctness err.
        let hash = gen_hash();
        let lock_hash = gen_hash();
        let lock = Lock {
            round: 1,
            hash:  lock_hash.clone(),
        };
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(2, Step::Propose, lock_hash.clone(), Some(lock)),
            SMRTrigger::new(hash, TriggerType::Proposal, Some(1)),
            SMREvent::PrevoteVote(lock_hash.clone()),
            Some(ConsensusError::CorrectnessErr("Fork".to_string())),
            Some((1, lock_hash)),
        ));

        for case in test_cases.into_iter() {
            println!("Proposal test {}/19", index);
            index += 1;
            trigger_test(
                case.base,
                case.input,
                case.output,
                case.err,
                case.should_lock,
            );
        }
        println!("Proposal test success");
    }

    #[test]
    fn test_prevote_trigger() {
        let mut index = 1;
        let mut test_cases: Vec<StateMachineTestCase> = Vec::new();

        // Test case 01: self proposal is not nil and not lock, prevote is equal to self proposal.
        // The output should be precommit vote to the prevote hash.
        let hash = gen_hash();
        test_cases.push(StateMachineTestCase::new(
            InnerState::new(0, Step::Prevote, Hash::new(), None),
            SMRTrigger::new(hash.clone(), TriggerType::PrevoteQC, Some(0)),
            SMREvent::PrecommitVote(hash.clone()),
            None,
            Some((0, hash)),
        ));

        for case in test_cases.into_iter() {
            println!("Prevote test {}/24", index);
            index += 1;
            trigger_test(
                case.base,
                case.input,
                case.output,
                case.err,
                case.should_lock,
            );
        }
    }
}
