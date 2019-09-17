mod event_test;
mod test_utils;

use bytes::Bytes;
use rand::random;

use crate::state::collection::{ProposalCollector, VoteCollector};
use crate::state::process::State;
use crate::state::tests::test_utils::{BlsCrypto, ConsensusHelper, Pill};
use crate::types::{Address, Hash, SignedVote, Vote, VoteType};
use crate::Codec;

#[derive(Debug)]
struct Condition<T: Codec> {
    epoch_id:           u64,
    round:              u64,
    proposal_collector: Option<ProposalCollector<T>>,
    vote_collector:     Option<VoteCollector>,
    is_full_txs:        bool,
}

impl<T: Codec> Condition<T> {
    fn new(
        epoch_id: u64,
        round: u64,
        proposal_collector: Option<ProposalCollector<T>>,
        vote_collector: Option<VoteCollector>,
        is_full_txs: bool,
    ) -> Self {
        Condition {
            epoch_id,
            round,
            proposal_collector,
            vote_collector,
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

fn update_state(
    info: &mut Condition<Pill>,
    state: &mut State<Pill, ConsensusHelper<Pill>, BlsCrypto>,
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
}
