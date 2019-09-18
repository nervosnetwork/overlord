use std::collections::HashMap;

use crossbeam_channel::unbounded;
use futures::channel::mpsc::unbounded as fut_unbounded;
use futures::StreamExt;
use tokio::runtime::Runtime;

use crate::smr::smr_types::{SMRTrigger, TriggerSource, TriggerType};
use crate::state::collection::VoteCollector;
use crate::state::process::State;
use crate::state::tests::test_utils::{BlsCrypto, ConsensusHelper, Pill};
use crate::types::{Address, OverlordMsg, VoteType};
use crate::{error::ConsensusError, smr::SMR, Codec};

use super::*;

struct MsgTestCase<T: Codec> {
    condition: Condition<T>,
    input:     OverlordMsg<T>,
    output:    Option<SMRTrigger>,
    err_value: Option<ConsensusError>,
}

impl<T: Codec> MsgTestCase<T> {
    fn new(
        condition: Condition<T>,
        input: OverlordMsg<T>,
        output: Option<SMRTrigger>,
        err_value: Option<ConsensusError>,
    ) -> Self {
        MsgTestCase {
            condition,
            input,
            output,
            err_value,
        }
    }

    fn flat(
        self,
    ) -> (
        Condition<T>,
        OverlordMsg<T>,
        Option<SMRTrigger>,
        Option<ConsensusError>,
    ) {
        (self.condition, self.input, self.output, self.err_value)
    }
}

fn handle_msg_test(
    mut condition: Condition<Pill>,
    input: OverlordMsg<Pill>,
    output: Option<SMRTrigger>,
    err_value: Option<ConsensusError>,
) {
    let (smr_tx, mut smr_rx) = fut_unbounded();
    let (msg_tx, _msg_rx) = unbounded();

    let smr = SMR::new(smr_tx);
    let address = Address::from(vec![0u8]);
    let helper = ConsensusHelper::new(msg_tx);
    let crypto = BlsCrypto::new(Address::from(vec![0u8]));

    let mut state = State::new(smr, address, 3000, helper, crypto);
    state.set_authority(gen_auth_list());
    update_state(&mut condition, &mut state);
    assert!(condition.proposal_collector.is_none());
    assert!(condition.vote_collector.is_none());
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let res = state.handle_msg(Some(input)).await;
        res.unwrap();
        // if let Some(err) = err_value {
        //     assert_eq!(res.err().unwrap(), err);
        //     return;
        // }

        if let Some(tmp) = output {
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
fn test_handle_msg() {
    let mut index = 1;
    let mut test_cases = Vec::new();

    let proposal = gen_signed_proposal(1, 0, None, 3);
    let input = OverlordMsg::SignedProposal(proposal);
    let condition = Condition::<Pill>::new(1, 0, None, None, None, false);
    let output = SMRTrigger {
        trigger_type: TriggerType::Proposal,
        source:       TriggerSource::State,
        hash:         epoch_hash(),
        round:        None,
    };
    test_cases.push(MsgTestCase::new(condition, input, Some(output), None));

    for case in test_cases.into_iter() {
        println!("Handle event test {}/1", index);
        let (condition, input, output, err) = case.flat();
        handle_msg_test(condition, input, output, err);
        index += 1;
    }
    println!("State handle message test success");
}
