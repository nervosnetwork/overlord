use std::sync::Arc;

use bytes::Bytes;
use futures::channel::mpsc::unbounded;
use futures::stream::StreamExt;
use rand::random;

use crate::smr::smr_types::{SMREvent, Step};
use crate::smr::{Event, SMR};
use crate::state::process::State;
use crate::types::{
    AggregatedVote, Commit, Node, OverlordMsg, PoLC, Proof, Proposal, SignedProposal, SignedVote,
    Vote, VoteType,
};
use crate::wal::{WalInfo, WalLock};

use super::util::*;

#[derive(Clone)]
struct WalTestCase {
    address:      Bytes,
    auth_list:    Vec<Node>,
    input:        WalInfo<Pill>,
    state_output: Option<OverlordMsg<Pill>>,
    smr_output:   Option<SMREvent>,
}

async fn test_process(case: WalTestCase) {
    let (overlord_tx, mut overlord_rx) = unbounded();
    let (raw_tx, raw_rx) = unbounded();
    let (mock_tx, mock_rx) = unbounded();
    let (mut smr, mut smr_event, _) = SMR::new();
    let handler = smr.take_smr();
    smr.run();

    let engine = ConsensusHelper::new(overlord_tx, case.auth_list.clone());
    let (mut overlord, tmp_rx) = State::new(
        handler,
        case.address.clone(),
        3000,
        case.auth_list.clone(),
        Arc::new(engine),
        MockCrypto,
        Arc::new(MockWal::new(case.input.clone())),
    );

    tokio::spawn(async move {
        let _ = overlord.run(raw_rx, Event::new(mock_rx), tmp_rx).await;
    });

    if let Some(output) = case.state_output {
        assert_eq!(overlord_rx.next().await.expect("state"), output);
    }

    if let Some(output) = case.smr_output {
        assert_eq!(smr_event.next().await.expect("smr"), output);
    }

    smr_event.close();
    overlord_rx.close();
    raw_tx.close_channel();
    mock_tx.close_channel();
}

#[tokio::test(threaded_scheduler)]
#[ignore]
async fn test_wal() {
    let mut test_cases = Vec::new();
    let mut index = 1;
    // case 01:
    // propose step
    // is leader
    // is not lock
    let auth_list = gen_auth_list(1);
    let wal = WalInfo {
        epoch_id: 0,
        round:    0,
        step:     Step::Propose,
        lock:     None,
    };
    let event = SMREvent::NewRoundInfo {
        epoch_id:      0,
        round:         0,
        lock_round:    None,
        lock_proposal: None,
    };
    let case_01 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: None,
        smr_output: Some(event),
    };
    test_cases.push(case_01);

    // case 02:
    // propose step
    // is not leader
    // is not lock
    let auth_list = gen_auth_list(2);
    let wal = WalInfo {
        epoch_id: 0,
        round:    0,
        step:     Step::Propose,
        lock:     None,
    };
    let event = SMREvent::NewRoundInfo {
        epoch_id:      0,
        round:         0,
        lock_round:    None,
        lock_proposal: None,
    };
    let case_02 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: None,
        smr_output: Some(event),
    };
    test_cases.push(case_02);

    // case 03:
    // propose step
    // is leader
    // is lock
    let auth_list = gen_auth_list(1);
    let qc = AggregatedVote {
        signature:  mock_aggregate_signature(),
        vote_type:  VoteType::Prevote,
        epoch_id:   0,
        round:      0,
        epoch_hash: Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
        leader:     auth_list[0].address.clone(),
    };
    let lock = WalLock {
        lock_round: 0,
        lock_votes: qc.clone(),
        content:    Pill::new(0),
    };
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Propose,
        lock:     Some(lock),
    };
    let proposal = Proposal {
        epoch_id:   0,
        round:      1,
        content:    Pill::new(0),
        epoch_hash: qc.epoch_hash.clone(),
        lock:       Some(PoLC {
            lock_round: 0,
            lock_votes: qc.clone(),
        }),
        proposer:   auth_list[0].address.clone(),
    };
    let msg = OverlordMsg::SignedProposal(SignedProposal {
        signature: Bytes::from(rlp::encode(&proposal)),
        proposal,
    });
    let event = SMREvent::NewRoundInfo {
        epoch_id:      0,
        round:         1,
        lock_round:    Some(0),
        lock_proposal: Some(qc.epoch_hash.clone()),
    };
    let case_03 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: Some(msg),
        smr_output: Some(event),
    };
    test_cases.push(case_03);

    // case 04:
    // propose step
    // is not leader
    // is lock
    let auth_list = gen_auth_list(2);
    let qc = AggregatedVote {
        signature:  mock_aggregate_signature(),
        vote_type:  VoteType::Prevote,
        epoch_id:   0,
        round:      0,
        epoch_hash: Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
        leader:     auth_list[0].address.clone(),
    };
    let lock = WalLock {
        lock_round: 0,
        lock_votes: qc.clone(),
        content:    Pill::new(0),
    };
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Propose,
        lock:     Some(lock),
    };
    let event = SMREvent::NewRoundInfo {
        epoch_id:      0,
        round:         1,
        lock_round:    Some(0),
        lock_proposal: Some(qc.epoch_hash.clone()),
    };
    let case_04 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: None,
        smr_output: Some(event),
    };
    test_cases.push(case_04);

    // case 05:
    // prevote step
    // is leader
    // is not lock
    let auth_list = gen_auth_list(1);
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Prevote,
        lock:     None,
    };
    let event = SMREvent::PrevoteVote {
        epoch_id:   0,
        round:      1,
        epoch_hash: Bytes::new(),
        lock_round: None,
    };
    let case_05 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: None,
        smr_output: Some(event),
    };
    test_cases.push(case_05);

    // case 06:
    // prevote step
    // is not leader
    // is not lock
    let auth_list = gen_auth_list(1);
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Prevote,
        lock:     None,
    };
    let event = SMREvent::PrevoteVote {
        epoch_id:   0,
        round:      1,
        epoch_hash: Bytes::new(),
        lock_round: None,
    };
    let case_06 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: None,
        smr_output: Some(event),
    };
    test_cases.push(case_06);

    // case 07:
    // prevote step
    // is not leader
    // is lock
    let auth_list = gen_auth_list(2);
    let qc = AggregatedVote {
        signature:  mock_aggregate_signature(),
        vote_type:  VoteType::Prevote,
        epoch_id:   0,
        round:      0,
        epoch_hash: Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
        leader:     auth_list[0].address.clone(),
    };
    let lock = WalLock {
        lock_round: 0,
        lock_votes: qc.clone(),
        content:    Pill::new(0),
    };
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Prevote,
        lock:     Some(lock),
    };
    let vote = Vote {
        epoch_id:   0,
        round:      1,
        vote_type:  VoteType::Prevote,
        epoch_hash: qc.epoch_hash.clone(),
    };
    let msg = OverlordMsg::SignedVote(SignedVote {
        signature: Bytes::from(rlp::encode(&vote)),
        vote,
        voter: auth_list[0].address.clone(),
    });
    let event = SMREvent::PrevoteVote {
        epoch_id:   0,
        round:      1,
        lock_round: Some(0),
        epoch_hash: Bytes::new(),
    };
    let case_07 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: Some(msg),
        smr_output: Some(event),
    };
    test_cases.push(case_07);

    // case 08:
    // prevote step
    // is leader
    // is lock
    let auth_list = gen_auth_list(2);
    let qc = AggregatedVote {
        signature:  mock_aggregate_signature(),
        vote_type:  VoteType::Prevote,
        epoch_id:   0,
        round:      0,
        epoch_hash: Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
        leader:     auth_list[0].address.clone(),
    };
    let lock = WalLock {
        lock_round: 0,
        lock_votes: qc.clone(),
        content:    Pill::new(0),
    };
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Prevote,
        lock:     Some(lock),
    };
    let event = SMREvent::PrevoteVote {
        epoch_id:   0,
        round:      1,
        lock_round: Some(0),
        epoch_hash: Bytes::new(),
    };
    let case_08 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: None,
        smr_output: Some(event),
    };
    test_cases.push(case_08);

    // case 09:
    // precommit step
    // is leader
    // is not lock
    let auth_list = gen_auth_list(1);
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Precommit,
        lock:     None,
    };
    let event = SMREvent::PrecommitVote {
        epoch_id:   0,
        round:      1,
        lock_round: None,
        epoch_hash: Bytes::new(),
    };
    let case_09 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: None,
        smr_output: Some(event),
    };
    test_cases.push(case_09);

    // case 10:
    // precommit step
    // is not leader
    // is not lock
    let auth_list = gen_auth_list(2);
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Precommit,
        lock:     None,
    };
    let event = SMREvent::PrecommitVote {
        epoch_id:   0,
        round:      1,
        lock_round: None,
        epoch_hash: Bytes::new(),
    };
    let case_10 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: None,
        smr_output: Some(event),
    };
    test_cases.push(case_10);

    // case 11:
    // prevote step
    // is not leader
    // is lock
    let auth_list = gen_auth_list(2);
    let qc = AggregatedVote {
        signature:  mock_aggregate_signature(),
        vote_type:  VoteType::Prevote,
        epoch_id:   0,
        round:      0,
        epoch_hash: Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
        leader:     auth_list[0].address.clone(),
    };
    let lock = WalLock {
        lock_round: 0,
        lock_votes: qc.clone(),
        content:    Pill::new(0),
    };
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Precommit,
        lock:     Some(lock),
    };
    let vote = Vote {
        epoch_id:   0,
        round:      1,
        vote_type:  VoteType::Prevote,
        epoch_hash: qc.epoch_hash.clone(),
    };
    let msg = OverlordMsg::SignedVote(SignedVote {
        signature: Bytes::from(rlp::encode(&vote)),
        vote,
        voter: auth_list[0].address.clone(),
    });
    let event = SMREvent::PrecommitVote {
        epoch_id:   0,
        round:      1,
        lock_round: Some(0),
        epoch_hash: Bytes::new(),
    };
    let case_11 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: Some(msg),
        smr_output: Some(event),
    };
    test_cases.push(case_11);

    // case 12:
    // prevote step
    // is leader
    // is lock
    let auth_list = gen_auth_list(1);
    let qc = AggregatedVote {
        signature:  mock_aggregate_signature(),
        vote_type:  VoteType::Prevote,
        epoch_id:   0,
        round:      0,
        epoch_hash: Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
        leader:     auth_list[0].address.clone(),
    };
    let lock = WalLock {
        lock_round: 0,
        lock_votes: qc.clone(),
        content:    Pill::new(0),
    };
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Precommit,
        lock:     Some(lock),
    };
    let event = SMREvent::PrecommitVote {
        epoch_id:   0,
        round:      1,
        lock_round: Some(0),
        epoch_hash: Bytes::new(),
    };
    let case_12 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: None,
        smr_output: Some(event),
    };
    test_cases.push(case_12);

    // case 13:
    // commit step
    // is not leader
    // is lock
    let auth_list = gen_auth_list(2);
    let sig = mock_aggregate_signature();
    let qc = AggregatedVote {
        signature:  sig.clone(),
        vote_type:  VoteType::Precommit,
        epoch_id:   0,
        round:      1,
        epoch_hash: Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
        leader:     auth_list[0].address.clone(),
    };
    let lock = WalLock {
        lock_round: 1,
        lock_votes: qc.clone(),
        content:    Pill::new(0),
    };
    let wal = WalInfo {
        epoch_id: 0,
        round:    1,
        step:     Step::Commit,
        lock:     Some(lock),
    };
    let proof = Proof {
        epoch_id:   0,
        round:      1,
        epoch_hash: qc.epoch_hash.clone(),
        signature:  sig,
    };
    let msg = OverlordMsg::Commit(Commit {
        epoch_id: 0,
        content: Pill::new(0),
        proof,
    });
    let case_13 = WalTestCase {
        address: auth_list[0].address.clone(),
        auth_list,
        input: wal,
        state_output: Some(msg),
        smr_output: None,
    };
    test_cases.push(case_13);

    for case in test_cases.into_iter() {
        println!("test {}", index);
        test_process(case).await;
        index += 1;
    }
}
