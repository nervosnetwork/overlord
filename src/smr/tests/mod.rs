/// Test new epoch trigger process.
mod new_epoch_test;
/// Test prevoteQC trigger process.
mod precommit_test;
/// Test prevoteQC trigger process.
mod prevote_test;
/// Test proposal trigger process.
mod proposal_test;

use rand::random;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::unbounded_channel;

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
    let (event_tx, mut event_rx) = unbounded_channel();
    let (state_tx, _state_rx) = unbounded_channel();

    let mut state_machine = StateMachine::new((event_tx, state_tx), trigger_rx);
    state_machine.set_status(base.round, base.step, base.proposal_hash, base.lock);
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
