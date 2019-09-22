use crate::smr::smr_types::{Lock, SMREvent, SMRTrigger, Step, TriggerType};
use crate::smr::tests::{gen_hash, trigger_test, InnerState, StateMachineTestCase};
use crate::{error::ConsensusError, types::Hash};

/// Test state machine handle proposal trigger.
/// There are a total of *4 × 4 + 3 = 19* test cases.
#[runtime::test]
async fn test_proposal_trigger() {
    let mut index = 1;
    let mut test_cases: Vec<StateMachineTestCase> = Vec::new();

    // Test case 01:
    //      self proposal is empty and self is not lock,
    //      proposal is not nil and no lock.
    // The output should be prevote vote to the proposal hash.
    let hash = gen_hash();
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(0, Step::Propose, Hash::new(), None),
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, None, 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      0u64,
            epoch_hash: hash,
        },
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
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, None, 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      0u64,
            epoch_hash: hash,
        },
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
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(0), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash,
        },
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
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(0), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash,
        },
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
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(0), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash,
        },
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
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, None, 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash,
        },
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
        SMRTrigger::new(Hash::new(), TriggerType::Proposal, Some(0), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash,
        },
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
        SMRTrigger::new(Hash::new(), TriggerType::Proposal, None, 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash,
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        None,
    ));

    // Test case 09:
    //      self proposal is empty and self is lock,
    //      proposal is nil and no lock.
    // This is an incorrect situation, the process cannot pass self check.
    let hash = Hash::new();
    let lock_hash = gen_hash();
    let lock = Lock::new(0, lock_hash);
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Propose, hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, None, 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash,
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        None,
    ));

    // Test case 10:
    //      self proposal is empty and self is lock,
    //      proposal is nil and with a lock.
    // This is an incorrect situation, the process cannot pass self check.
    let hash = Hash::new();
    let lock_hash = gen_hash();
    let lock = Lock::new(0, lock_hash);
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Propose, hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(0), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash,
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        None,
    ));

    // Test case 11:
    //      self proposal is empty and self is lock,
    //      proposal is not nil and no lock.
    // This is an incorrect situation, the process cannot pass self check.
    let hash = Hash::new();
    let lock_hash = gen_hash();
    let lock = Lock::new(0, lock_hash);
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Propose, hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, None, 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: hash,
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        None,
    ));

    // Test case 12:
    //      self proposal is empty and self is lock,
    //      proposal is not nil and with a lock.
    // This is an incorrect situation, the process cannot pass self check.
    let hash = Hash::new();
    let lock_hash = gen_hash();
    let lock = Lock::new(0, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Propose, hash, Some(lock)),
        SMRTrigger::new(lock_hash.clone(), TriggerType::Proposal, None, 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: lock_hash,
        },
        Some(ConsensusError::SelfCheckErr("".to_string())),
        None,
    ));

    // Test case 13:
    //      self proposal is not empty and self is lock,
    //      proposal is nil and no lock.
    // The output should be prevote vote to the self lock hash.
    let hash = Hash::new();
    let lock_hash = gen_hash();
    let lock = Lock::new(0, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Propose, lock_hash.clone(), Some(lock)),
        SMRTrigger::new(hash, TriggerType::Proposal, None, 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: lock_hash.clone(),
        },
        None,
        Some((0, lock_hash)),
    ));

    // Test case 14:
    //      self proposal is not empty and self is lock,
    //      proposal is not nil and no lock.
    // The output should be prevote vote to the self lock hash.
    let hash = gen_hash();
    let lock_hash = gen_hash();
    let lock = Lock::new(0, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Propose, lock_hash.clone(), Some(lock)),
        SMRTrigger::new(hash, TriggerType::Proposal, None, 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: lock_hash.clone(),
        },
        None,
        Some((0, lock_hash)),
    ));

    // Test case 15:
    //      self proposal is not empty and self is lock,
    //      proposal is nil but with a lock.
    // This is an incorrect situation, the process will return proposal err.
    let hash = Hash::new();
    let lock_hash = gen_hash();
    let lock = Lock::new(0, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(1, Step::Propose, lock_hash.clone(), Some(lock)),
        SMRTrigger::new(hash, TriggerType::Proposal, Some(0), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      1u64,
            epoch_hash: lock_hash.clone(),
        },
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
    let lock = Lock::new(1, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(2, Step::Propose, lock_hash.clone(), Some(lock)),
        SMRTrigger::new(hash, TriggerType::Proposal, Some(0), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      2u64,
            epoch_hash: lock_hash.clone(),
        },
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
    let lock = Lock::new(1, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(3, Step::Propose, lock_hash.clone(), Some(lock)),
        SMRTrigger::new(hash.clone(), TriggerType::Proposal, Some(2), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      3u64,
            epoch_hash: hash,
        },
        None,
        None,
    ));

    // Test case 18:
    //      self proposal is not empty and self is lock,
    //      proposal is not nil and with a lock.
    //      proposal lock round and proposal is equal to self lock round and proposal.
    // The output should be prevote vote to the self lock hash.
    let lock_hash = gen_hash();
    let lock = Lock::new(1, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(2, Step::Propose, lock_hash.clone(), Some(lock)),
        SMRTrigger::new(lock_hash.clone(), TriggerType::Proposal, Some(1), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      2u64,
            epoch_hash: lock_hash.clone(),
        },
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
    let lock = Lock::new(1, lock_hash.clone());
    test_cases.push(StateMachineTestCase::new(
        InnerState::new(2, Step::Propose, lock_hash.clone(), Some(lock)),
        SMRTrigger::new(hash, TriggerType::Proposal, Some(1), 0),
        SMREvent::PrevoteVote {
            epoch_id:   0u64,
            round:      2u64,
            epoch_hash: lock_hash.clone(),
        },
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
        )
        .await;
    }
    println!("Proposal test success");
}
