use std::collections::HashMap;
use std::sync::Arc;

use crossbeam_channel::unbounded;
use futures::channel::mpsc::unbounded as fut_unbounded;
use futures::StreamExt;

use crate::smr::smr_types::{SMREvent, SMRTrigger};
use crate::state::collection::VoteCollector;
use crate::state::process::State;
use crate::state::tests::test_utils::{BlsCrypto, ConsensusHelper, Pill};
use crate::types::{Address, OverlordMsg, VoteType};
use crate::{smr::SMRHandler, Codec};

use super::*;

struct EventTestCase<T: Codec> {
    condition:  Condition<T>,
    input:      SMREvent,
    output_msg: OverlordMsg<T>,
    output_smr: Option<SMRTrigger>,
}

impl<T: Codec> EventTestCase<T> {
    fn new(
        condition: Condition<T>,
        input: SMREvent,
        output_msg: OverlordMsg<T>,
        output_smr: Option<SMRTrigger>,
    ) -> Self {
        EventTestCase {
            input,
            condition,
            output_msg,
            output_smr,
        }
    }

    fn flat(self) -> (Condition<T>, SMREvent, OverlordMsg<T>, Option<SMRTrigger>) {
        (self.condition, self.input, self.output_msg, self.output_smr)
    }
}

async fn handle_event_test(
    mut condition: Condition<Pill>,
    input: SMREvent,
    output_msg: OverlordMsg<Pill>,
    output_smr: Option<SMRTrigger>,
) {
    let (smr_tx, mut smr_rx) = fut_unbounded();
    let (msg_tx, msg_rx) = unbounded();

    let smr_handler = SMRHandler::new(smr_tx);
    let address = Address::from(vec![0u8]);
    let helper = ConsensusHelper::new(msg_tx);
    let crypto = BlsCrypto::new(Address::from(vec![0u8]));

    let (mut state, _rx) = State::new(smr_handler, address, 3000, Arc::new(helper), crypto);
    update_state(&mut condition, &mut state);
    assert!(condition.proposal_collector.is_none());
    assert!(condition.vote_collector.is_none());

    state.handle_event(Some(input)).await.unwrap();
    assert_eq!(msg_rx.recv().unwrap(), output_msg);

    if let Some(tmp) = output_smr {
        loop {
            match smr_rx.next().await {
                Some(res) => {
                    assert_eq!(res, tmp);
                    return;
                }
                None => continue,
            }
        }
    }
}

#[runtime::test]
async fn test_handle_event() {
    let mut index = 1;
    let mut test_cases = Vec::new();

    // Test case 01:
    // Test state handle prevote vote event.
    let input = SMREvent::PrevoteVote {
        epoch_id:   1u64,
        round:      0u64,
        epoch_hash: epoch_hash(),
    };
    let condition = Condition::<Pill>::new(1, 0, None, None, None, true);
    let output_msg = gen_signed_vote(1, 0, VoteType::Prevote, epoch_hash());
    test_cases.push(EventTestCase::new(
        condition,
        input,
        OverlordMsg::SignedVote(output_msg),
        None,
    ));

    // Test case 02:
    // Test state handle precommit vote event.
    let input = SMREvent::PrecommitVote {
        epoch_id:   1u64,
        round:      0u64,
        epoch_hash: epoch_hash(),
    };
    let condition = Condition::<Pill>::new(1, 0, None, None, None, true);
    let output_msg = gen_signed_vote(1, 0, VoteType::Precommit, epoch_hash());
    test_cases.push(EventTestCase::new(
        condition,
        input,
        OverlordMsg::SignedVote(output_msg),
        None,
    ));

    // Test case 03:
    // Test state handle commit event.
    let mut hash_with_epoch = HashMap::new();
    hash_with_epoch.insert(epoch_hash(), Pill::new(1u64));
    let mut votes = VoteCollector::new();
    let signature = gen_signature(255);
    votes.set_qc(gen_aggregated_vote(
        1,
        0,
        signature.clone(),
        VoteType::Precommit,
        epoch_hash(),
        Address::from(vec![0u8]),
    ));
    let input = SMREvent::Commit(epoch_hash());
    let condition = Condition::<Pill>::new(1, 0, None, Some(votes), Some(hash_with_epoch), true);
    let output_msg = gen_commit(1, 0, signature);
    test_cases.push(EventTestCase::new(
        condition,
        input,
        OverlordMsg::Commit(output_msg),
        None,
    ));

    for case in test_cases.into_iter() {
        println!("Handle event test {}/3", index);
        let (condition, input, output_msg, output_smr) = case.flat();
        handle_event_test(condition, input, output_msg, output_smr).await;
        index += 1;
    }
    println!("State handle event test success");
}
