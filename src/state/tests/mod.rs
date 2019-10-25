#![allow(dead_code)]

mod event_test;
mod test_utils;

use std::collections::HashMap;

use bit_vec::BitVec;
use bytes::Bytes;
use rand::random;

use crate::state::collection::{ProposalCollector, VoteCollector};
use crate::state::process::State;
use crate::state::tests::test_utils::{BlsCrypto, ConsensusHelper, Pill};
use crate::types::{
    Address, AggregatedSignature, AggregatedVote, Commit, Hash, Node, PoLC, Proof, Proposal,
    Signature, SignedProposal, SignedVote, Vote, VoteType,
};
use crate::Codec;

#[derive(Debug)]
struct Condition<T: Codec> {
    epoch_id:           u64,
    round:              u64,
    proposal_collector: Option<ProposalCollector<T>>,
    vote_collector:     Option<VoteCollector>,
    hash_with_epoch:    Option<HashMap<Hash, T>>,
    is_full_txs:        bool,
}

impl<T: Codec> Condition<T> {
    fn new(
        epoch_id: u64,
        round: u64,
        proposal_collector: Option<ProposalCollector<T>>,
        vote_collector: Option<VoteCollector>,
        hash_with_epoch: Option<HashMap<Hash, T>>,
        is_full_txs: bool,
    ) -> Self {
        Condition {
            epoch_id,
            round,
            proposal_collector,
            vote_collector,
            hash_with_epoch,
            is_full_txs,
        }
    }
}

fn gen_hash() -> Hash {
    Hash::from((0..5).map(|_| random::<u8>()).collect::<Vec<_>>())
}

fn epoch_hash() -> Hash {
    Hash::from(vec![1, 2, 3, 4, 5])
}

fn gen_signature(i: u8) -> Signature {
    Signature::from(vec![i])
}

fn gen_signed_proposal(
    epoch_id: u64,
    round: u64,
    lock: Option<PoLC>,
    proposer: u8,
) -> SignedProposal<Pill> {
    let signature = gen_signature(proposer);
    let proposal = Proposal {
        epoch_id,
        round,
        content: Pill::new(epoch_id),
        epoch_hash: epoch_hash(),
        lock,
        proposer: Address::from(vec![proposer]),
    };

    SignedProposal {
        signature,
        proposal,
    }
}

fn gen_aggregated_vote(
    epoch_id: u64,
    round: u64,
    signature: Signature,
    vote_type: VoteType,
    epoch_hash: Hash,
    leader: Address,
) -> AggregatedVote {
    let bitmap = BitVec::from_elem(4, true);
    let signature = AggregatedSignature {
        signature,
        address_bitmap: Bytes::from(bitmap.to_bytes()),
    };

    AggregatedVote {
        signature,
        vote_type,
        epoch_id,
        round,
        epoch_hash,
        leader,
    }
}

fn gen_signed_vote(epoch_id: u64, round: u64, vote_type: VoteType, hash: Hash) -> SignedVote {
    let signature = Bytes::from(vec![0u8]);
    let vote = Vote {
        epoch_id,
        round,
        vote_type,
        epoch_hash: hash,
        voter: Address::from(vec![0u8]),
    };
    SignedVote { signature, vote }
}

fn gen_commit(epoch_id: u64, round: u64, signature: Signature) -> Commit<Pill> {
    let content = Pill::new(epoch_id);
    let epoch_hash = epoch_hash();
    let bitmap = BitVec::from_elem(4, true);
    let signature = AggregatedSignature {
        signature,
        address_bitmap: Bytes::from(bitmap.to_bytes()),
    };
    let proof = Proof {
        epoch_id,
        round,
        epoch_hash,
        signature,
    };

    Commit {
        epoch_id,
        content,
        proof,
    }
}

fn gen_auth_list() -> Vec<Node> {
    let tmp = vec![0, 1, 2, 3];
    let mut res = tmp
        .iter()
        .map(|i| Node::new(Address::from(vec![*i as u8])))
        .collect::<Vec<_>>();
    res.sort();
    res
}

fn update_state(
    info: &mut Condition<Pill>,
    state: &mut State<Pill, Pill, ConsensusHelper<Pill>, BlsCrypto>,
) {
    state.set_condition(info.epoch_id, info.round);

    if info.proposal_collector.is_some() {
        state.set_proposal_collector(info.proposal_collector.take().unwrap());
    }

    if info.vote_collector.is_some() {
        state.set_vote_collector(info.vote_collector.take().unwrap());
    }

    if info.is_full_txs {
        state.set_full_transaction(Hash::from(vec![1u8, 2, 3, 4, 5]));
    }

    if info.hash_with_epoch.is_some() {
        state.set_hash_with_epoch(info.hash_with_epoch.take().unwrap());
    }
}
