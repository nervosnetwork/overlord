use crossbeam_channel::unbounded;
use futures::channel::mpsc::unbounded as fut_unbounded;
use futures::StreamExt;
use tokio::runtime::Runtime;

use crate::smr::smr_types::{SMREvent, SMRTrigger};
use crate::state::process::State;
use crate::state::tests::test_utils::{BlsCrypto, ConsensusHelper, Pill};
use crate::state::tests::{epoch_hash, gen_signed_vote, update_state, Condition};
use crate::types::{Address, Hash, OverlordMsg, VoteType};
use crate::{smr::SMR, Codec};

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

fn handle_event_test(
    mut condition: Condition<Pill>,
    input: SMREvent,
    output_msg: OverlordMsg<Pill>,
    output_smr: Option<SMRTrigger>,
) {
    let (smr_tx, mut smr_rx) = fut_unbounded();
    let (msg_tx, msg_rx) = unbounded();
    let (commit_tx, _commit_rx) = unbounded();

    let smr = SMR::new(smr_tx);
    let address = Address::from(vec![0u8]);
    let helper = ConsensusHelper::new(msg_tx, commit_tx);
    let crypto = BlsCrypto::new(Address::from(vec![0u8]));

    let mut state = State::new(smr, address, 3000, helper, crypto);
    update_state(&mut condition, &mut state);
    assert!(condition.proposal_collector.is_none());
    assert!(condition.vote_collector.is_none());
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let _ = state.handle_event(Some(input)).await;
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
    })
}

#[test]
fn test_handle_event() {
    let mut index = 1;
    let mut test_cases = Vec::new();

    let input = SMREvent::PrevoteVote(Hash::from(vec![1, 2, 3, 4, 5]));
    let condition = Condition::<Pill>::new(1, 0, None, None, true);
    let output_msg = gen_signed_vote(1, 0, VoteType::Prevote, epoch_hash());
    test_cases.push(EventTestCase::new(
        condition,
        input,
        OverlordMsg::SignedVote(output_msg),
        None,
    ));

    for case in test_cases.into_iter() {
        println!("Handle event test {}/16", index);
        index += 1;
        let (condition, input, output_msg, output_smr) = case.flat();
        handle_event_test(condition, input, output_msg, output_smr);
    }
}
