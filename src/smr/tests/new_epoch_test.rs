use crate::smr::smr_types::{Lock, SMREvent, SMRTrigger, Step, TriggerType};
use crate::smr::tests::{gen_hash, trigger_test, InnerState, StateMachineTestCase};
use crate::{error::ConsensusError, types::Hash};

#[test]
fn test_new_epoch() {
    let mut index = 1;
    let mut test_cases: Vec<StateMachineTestCase> = Vec::new();

    // Test case 01:
    //      In propose step, self proposal is empty and no lock, goto new epoch.
    // The output should be new round info.
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Propose, Hash::new(), None),
        SMRTrigger::new(Hash::new(), TriggerType::NewEpoch(1), None, 0),
        SMREvent::NewRoundInfo {
            epoch_id:      1u64,
            round:         0u64,
            lock_round:    None,
            lock_proposal: None,
        },
        None,
        None,
    ));

    // Test case 02:
    //      In propose step, self proposal is not empty and with a lock, goto new epoch.
    // The output should be new round info.
    let hash = gen_hash();
    let lock = Lock {
        round: 0u64,
        hash:  hash.clone(),
    };
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Propose, hash, Some(lock)),
        SMRTrigger::new(Hash::new(), TriggerType::NewEpoch(1), None, 0),
        SMREvent::NewRoundInfo {
            epoch_id:      1u64,
            round:         0u64,
            lock_round:    None,
            lock_proposal: None,
        },
        None,
        None,
    ));

    // Test case 03:
    //      In propose step, self proposal is empty but with a lock, goto new epoch.
    // This is an incorrect situation, the process cannot pass self check.
    let hash = gen_hash();
    let lock = Lock::new(0u64, hash);
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Propose, Hash::new(), Some(lock)),
        SMRTrigger::new(Hash::new(), TriggerType::NewEpoch(1), None, 0),
        SMREvent::NewRoundInfo {
            epoch_id:      1u64,
            round:         0u64,
            lock_round:    None,
            lock_proposal: None,
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        None,
    ));

    // Test case 04:
    //      In propose step, self proposal is not empty and not lock, goto new epoch.
    // This is an incorrect situation, the process cannot pass self check.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Propose, hash, None),
        SMRTrigger::new(Hash::new(), TriggerType::NewEpoch(1), None, 0),
        SMREvent::NewRoundInfo {
            epoch_id:      1u64,
            round:         0u64,
            lock_round:    None,
            lock_proposal: None,
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        None,
    ));

    // Test case 05:
    //      In prevote step, self proposal is empty and not lock, goto new epoch.
    // The output should be new round info.
    let hash = Hash::new();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Prevote, hash, None),
        SMRTrigger::new(Hash::new(), TriggerType::NewEpoch(1), None, 0),
        SMREvent::NewRoundInfo {
            epoch_id:      1u64,
            round:         0u64,
            lock_round:    None,
            lock_proposal: None,
        },
        None,
        None,
    ));

    // Test case 06:
    //      In prevote step, self proposal is not empty and not lock, goto new epoch.
    // The output should be new round info.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Prevote, hash, None),
        SMRTrigger::new(Hash::new(), TriggerType::NewEpoch(1), None, 0),
        SMREvent::NewRoundInfo {
            epoch_id:      1u64,
            round:         0u64,
            lock_round:    None,
            lock_proposal: None,
        },
        None,
        None,
    ));

    // Test case 07:
    //      In prevote step, self proposal is not empty and not lock, goto new epoch.
    // The output should be new round info.
    let hash = gen_hash();
    let lock = Lock::new(0u64, hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Prevote, hash, Some(lock)),
        SMRTrigger::new(Hash::new(), TriggerType::NewEpoch(1), None, 0),
        SMREvent::NewRoundInfo {
            epoch_id:      1u64,
            round:         0u64,
            lock_round:    None,
            lock_proposal: None,
        },
        None,
        None,
    ));

    for case in test_cases.into_iter() {
        println!("New epoch test {}/16", index);
        index += 1;
        trigger_test(
            case.base,
            case.input,
            case.output,
            case.err,
            case.should_lock,
        );
    }
    println!("New epoch test success");
}
