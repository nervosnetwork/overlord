use crate::smr::smr_types::{Lock, SMREvent, SMRTrigger, Step, TriggerType};
use crate::smr::tests::{gen_hash, trigger_test, InnerState, StateMachineTestCase};
use crate::{error::ConsensusError, types::Hash};

/// Test state machine handle precommitQC trigger.
/// There are a total of *2 × 4 + 3 = 11* test cases.
#[tokio::test(threaded_scheduler)]
async fn test_precommit_trigger() {
    let mut index = 1;
    let mut test_cases: Vec<StateMachineTestCase> = Vec::new();

    // Test case 01:
    //      self proposal is not empty and not lock, precommit is not nil.
    // The output should be commit to the precommit hash.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, Hash::new(), None),
        SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(0), 0),
        SMREvent::Commit(hash.clone()),
        None,
        Some((0, hash)),
    ));

    // Test case 02:
    //      self proposal is not empty and not lock, precommit is nil.
    // The output should be new round info without lock.
    let hash = Hash::new();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, Hash::new(), None),
        SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(0), 0),
        SMREvent::NewRoundInfo {
            height:        0u64,
            round:         1u64,
            lock_round:    None,
            lock_proposal: None,
            new_interval:  None,
            new_config:    None,
        },
        Some(ConsensusError::PrecommitErr("Empty qc".to_string())),
        None,
    ));

    // Test case 03:
    //      self proposal is not empty and with a lock, precommit is not nil.
    // The output should be precommit vote to the precommit hash.
    let hash = gen_hash();
    let lock = Lock::new(0, hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(0), 0),
        SMREvent::Commit(hash.clone()),
        None,
        Some((0, hash)),
    ));

    // Test case 04:
    //      self proposal is not empty and with a lock, precommit is nil.
    // The output should be new round info with a lock.
    let hash = gen_hash();
    let lock = Lock {
        round: 0,
        hash:  hash.clone(),
    };
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, hash.clone(), Some(lock)),
        SMRTrigger::new(Hash::new(), TriggerType::PrecommitQC, Some(0), 0),
        SMREvent::NewRoundInfo {
            height:        0u64,
            round:         1u64,
            lock_round:    Some(0),
            lock_proposal: Some(hash.clone()),
            new_interval:  None,
            new_config:    None,
        },
        Some(ConsensusError::PrecommitErr("Empty qc".to_string())),
        Some((0, hash)),
    ));

    // Test case 05:
    //      self proposal is not empty and with a lock, precommit is nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = gen_hash();
    let lock = Lock::new(0, hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, Hash::new(), Some(lock)),
        SMRTrigger::new(Hash::new(), TriggerType::PrecommitQC, Some(0), 0),
        SMREvent::NewRoundInfo {
            height:        0u64,
            round:         1u64,
            lock_round:    Some(0),
            lock_proposal: Some(hash.clone()),
        },
        None,
        Some((0, hash)),
    ));

    // Test case 06:
    //      self proposal is not empty and with a lock, precommit is not nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = gen_hash();
    let lock = Lock::new(0, hash.clone());
    let hash_new = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, Hash::new(), Some(lock)),
        SMRTrigger::new(hash_new.clone(), TriggerType::PrecommitQC, Some(0), 0),
        SMREvent::Commit(hash_new.clone()),
        Some(ConsensusError::SelfCheckErr("".to_string())),
        Some((0, hash_new)),
    ));

    // Test case 07:
    //      self proposal is not empty and without lock, precommit is not nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, hash.clone(), None),
        SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(0), 0),
        SMREvent::PrecommitVote {
            height:     0u64,
            round:      0u64,
            block_hash: hash.clone(),
            lock_round: Some(0),
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        Some((0, hash)),
    ));

    // Test case 08:
    //      self proposal is not empty and without lock, precommit is nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, hash.clone(), None),
        SMRTrigger::new(Hash::new(), TriggerType::PrecommitQC, Some(0), 0),
        SMREvent::NewRoundInfo {
            height:        0u64,
            round:         1u64,
            lock_round:    None,
            lock_proposal: None,
            new_interval:  None,
            new_config:    None,
        },
        None,
        None,
    ));

    // // Test case 10:
    // //      the precommit round is not equal to self round.
    // // This is an incorrect situation, the process will return round diff err.
    // let hash = gen_hash();
    // let lock = Lock::new(0, hash.clone());
    // test_cases.push(StateMachineTestCase::new(
    //     InnerState::new(1, Step::Precommit, hash.clone(), Some(lock)),
    //     SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(0), 0),
    //     SMREvent::Commit(hash.clone()),
    //     Some(ConsensusError::RoundDiff { local: 1, vote: 0 }),
    //     Some((0, hash)),
    // ));

    // Test case 11:
    //      self proposal is not empty and with a lock, precommit is not nil. However, precommit
    //      hash is not equal to self lock hash.
    // This is extremely dangerous because it can lead to fork. The process will return
    // correctness err.
    let hash = gen_hash();
    let lock = Lock::new(0, hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, hash.clone(), Some(lock)),
        SMRTrigger::new(gen_hash(), TriggerType::PrecommitQC, Some(0), 0),
        SMREvent::Commit(hash.clone()),
        Some(ConsensusError::CorrectnessErr("Fork".to_string())),
        Some((0, hash)),
    ));

    for case in test_cases.into_iter() {
        println!("Precommit test {}/9", index);
        index += 1;
        trigger_test(
            case.base,
            case.input,
            case.output,
            case.err,
            case.should_lock,
        )
        .await;
    }
    println!("Precommit test success");
}
