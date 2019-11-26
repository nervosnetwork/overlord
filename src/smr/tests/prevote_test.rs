use crate::smr::smr_types::{Lock, SMREvent, SMRTrigger, Step, TriggerType};
use crate::smr::tests::{gen_hash, trigger_test, InnerState, StateMachineTestCase};
use crate::{error::ConsensusError, types::Hash};

/// Test state machine handle prevoteQC trigger.
/// There are a total of *2 × 4 + 2 = 10* test cases.
#[runtime::test]
async fn test_prevote_trigger() {
    let mut index = 1;
    let mut test_cases: Vec<StateMachineTestCase> = Vec::new();

    // Test case 01:
    //      self proposal is not empty and not lock, prevote is not nil.
    // The output should be precommit vote to the prevote hash.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Prevote, Hash::new(), None),
        SMRTrigger::new(hash.clone(), TriggerType::PrevoteQC, Some(0), 0),
        SMREvent::PrecommitVote {
            epoch_id:   0u64,
            round:      0u64,
            epoch_hash: hash.clone(),
        },
        None,
        Some((0, hash)),
    ));

    // Test case 02:
    //      self proposal is not empty and not lock, prevote is nil.
    // The output should be precommit vote to the prevote hash.
    let hash = Hash::new();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Prevote, Hash::new(), None),
        SMRTrigger::new(hash.clone(), TriggerType::PrevoteQC, Some(0), 0),
        SMREvent::PrecommitVote {
            epoch_id:   0u64,
            round:      0u64,
            epoch_hash: hash,
        },
        Some(ConsensusError::PrevoteErr("Empty qc".to_string())),
        None,
    ));

    // Test case 03:
    //      self proposal is not empty but with a lock, prevote is nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = Hash::new();
    let lock_hash = gen_hash();
    let lock = Lock {
        round: 0,
        hash:  lock_hash.clone(),
    };
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Prevote, hash.clone(), Some(lock)),
        SMRTrigger::new(hash, TriggerType::PrevoteQC, Some(1), 0),
        SMREvent::PrecommitVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: lock_hash.clone(),
        },
        Some(ConsensusError::PrevoteErr("Empty qc".to_string())),
        Some((0, lock_hash)),
    ));

    // Test case 04:
    //      self proposal is not empty but with a lock, prevote is not nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = gen_hash();
    let lock_hash = gen_hash();
    let lock = Lock {
        round: 0,
        hash:  lock_hash.clone(),
    };
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Prevote, Hash::new(), Some(lock)),
        SMRTrigger::new(hash, TriggerType::PrevoteQC, Some(1), 0),
        SMREvent::PrecommitVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: lock_hash.clone(),
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        Some((0, lock_hash)),
    ));

    // Test case 05:
    //      self proposal is not empty and no lock, prevote is nil.
    // The output should be precommit vote to the prevote hash which is nil.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Prevote, hash.clone(), None),
        SMRTrigger::new(Hash::new(), TriggerType::PrevoteQC, Some(1), 0),
        SMREvent::PrecommitVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: Hash::new(),
        },
        Some(ConsensusError::PrevoteErr("Empty qc".to_string())),
        None,
    ));

    // Test case 06:
    //      self proposal is not empty and no lock, prevote is not nil.
    // The output should be precommit vote to the prevote hash and lock on it.
    let hash = gen_hash();
    let vote_hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Prevote, hash.clone(), None),
        SMRTrigger::new(vote_hash.clone(), TriggerType::PrevoteQC, Some(1), 0),
        SMREvent::PrecommitVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: vote_hash.clone(),
        },
        None,
        Some((1, vote_hash)),
    ));

    // Test case 07:
    //      self proposal is not empty but with a lock, prevote is nil.
    // The output should be prevote vote to the nil hash.
    let hash = Hash::new();
    let lock_hash = gen_hash();
    let lock = Lock::new(0, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Prevote, lock_hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::PrevoteQC, Some(1), 0),
        SMREvent::PrecommitVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash.clone(),
        },
        Some(ConsensusError::PrevoteErr("Empty qc".to_string())),
        None,
    ));

    // Test case 08:
    //      self proposal is not empty but with a lock, prevote is not nil.
    // The output should be prevote vote to the prevote hash and lock it.
    let hash = gen_hash();
    let lock_hash = gen_hash();
    let lock = Lock::new(0, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Prevote, lock_hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::PrevoteQC, Some(1), 0),
        SMREvent::PrecommitVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash.clone(),
        },
        None,
        Some((1, hash)),
    ));

    // // Test case 09:
    // //      the prevote round is not equal to self round.
    // // This is an incorrect situation, the process will return round diff err.
    // let hash = gen_hash();
    // test_cases.push(StateMachineTestCase::new(
    //     InnerState::new(1, Step::Prevote, Hash::new(), None),
    //     SMRTrigger::new(hash, TriggerType::PrevoteQC, Some(0), 0),
    //     SMREvent::PrecommitVote {
    //         epoch_id:   0u64,
    //         round:      1u64,
    //         epoch_hash: Hash::new(),
    //     },
    //     Some(ConsensusError::RoundDiff { local: 1, vote: 0 }),
    //     None,
    // ));

    for case in test_cases.into_iter() {
        println!("Prevote test {}/8", index);
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
    println!("Prevote test success");
}
