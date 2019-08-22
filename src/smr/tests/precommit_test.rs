use crate::smr::smr_types::{Lock, SMREvent, SMRTrigger, Step, TriggerType};
use crate::smr::tests::{gen_hash, trigger_test, InnerState, StateMachineTestCase};
use crate::{error::ConsensusError, types::Hash};

#[test]
fn test_precommit_trigger() {
    let mut index = 1;
    let mut test_cases: Vec<StateMachineTestCase> = Vec::new();

    // Test case 01:
    //      self proposal is not empty and not lock, precommit is not nil.
    // The output should be commit to the precommit hash.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, Hash::new(), None),
        SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(0)),
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
        SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(0)),
        SMREvent::NewRoundInfo {
            round:         1u64,
            lock_round:    None,
            lock_proposal: None,
        },
        None,
        None,
    ));

    // Test case 03:
    //      self proposal is not empty and with a lock, prevote is not nil.
    // The output should be precommit vote to the prevote hash.
    let hash = gen_hash();
    let lock = Lock {
        round: 0u64,
        hash:  hash.clone(),
    };
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(0)),
        SMREvent::Commit(hash.clone()),
        None,
        Some((0, hash)),
    ));

    // Test case 04:
    //      self proposal is not empty and with a lock, precommit is nil.
    // The output should be new round info with a lock.
    let hash = gen_hash();
    let lock = Lock {
        round: 0u64,
        hash:  hash.clone(),
    };
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, hash.clone(), Some(lock)),
        SMRTrigger::new(Hash::new(), TriggerType::PrecommitQC, Some(0)),
        SMREvent::NewRoundInfo {
            round:         1u64,
            lock_round:    Some(0),
            lock_proposal: Some(hash.clone()),
        },
        None,
        Some((0, hash)),
    ));

    // Test case 05:
    //      self proposal is not empty and with a lock, precommit is nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = gen_hash();
    let lock = Lock {
        round: 0u64,
        hash:  hash.clone(),
    };
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, Hash::new(), Some(lock)),
        SMRTrigger::new(Hash::new(), TriggerType::PrecommitQC, Some(0)),
        SMREvent::Commit(hash.clone()),
        Some(ConsensusError::SelfCheckErr("".to_string())),
        Some((0, hash)),
    ));

    // Test case 06:
    //      self proposal is not empty and with a lock, precommit is not nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = gen_hash();
    let lock = Lock {
        round: 0u64,
        hash:  hash.clone(),
    };
    let hash_new = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, Hash::new(), Some(lock)),
        SMRTrigger::new(hash_new.clone(), TriggerType::PrecommitQC, Some(0)),
        SMREvent::Commit(hash_new.clone()),
        Some(ConsensusError::SelfCheckErr("".to_string())),
        Some((0, hash_new)),
    ));

    // Test case 07:
    //      self proposal is not empty and without lock, prevote is not nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Prevote, hash.clone(), None),
        SMRTrigger::new(hash.clone(), TriggerType::PrevoteQC, Some(0)),
        SMREvent::PrecommitVote(hash.clone()),
        Some(ConsensusError::SelfCheckErr("".to_string())),
        Some((0, hash)),
    ));

    // Test case 08:
    //      self proposal is not empty and without lock, prevote is nil.
    // This is an incorrect situation, the process can not pass self check.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, hash.clone(), None),
        SMRTrigger::new(Hash::new(), TriggerType::PrecommitQC, Some(0)),
        SMREvent::NewRoundInfo {
            round:         1u64,
            lock_round:    None,
            lock_proposal: None,
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        Some((0, hash)),
    ));

    // Test case 09:
    //      the precommit round is not equal to self round.
    // This is an incorrect situation, the process will return round diff err.
    let hash = gen_hash();
    let lock = Lock {
        round: 0u64,
        hash:  hash.clone(),
    };
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Precommit, hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(1)),
        SMREvent::Commit(hash.clone()),
        Some(ConsensusError::RoundDiff { local: 0, vote: 1 }),
        Some((0, hash)),
    ));

    // Test case 10:
    //      the precommit round is not equal to self round.
    // This is an incorrect situation, the process will return round diff err.
    let hash = gen_hash();
    let lock = Lock {
        round: 0u64,
        hash:  hash.clone(),
    };
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Precommit, hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::PrecommitQC, Some(0)),
        SMREvent::Commit(hash.clone()),
        Some(ConsensusError::RoundDiff { local: 1, vote: 0 }),
        Some((0, hash)),
    ));

    for case in test_cases.into_iter() {
        println!("Prevote test {}/10", index);
        index += 1;
        trigger_test(
            case.base,
            case.input,
            case.output,
            case.err,
            case.should_lock,
        );
    }
    println!("Prevote test success");
}
