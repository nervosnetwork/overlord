#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};

use bytes::Bytes;
use derive_more::Display;

use crate::auth::AuthManage;
use crate::types::{
    ChokeQC, CumWeight, FetchedFullBlock, PreCommitQC, PreVoteQC, Proposal, SignedChoke,
    SignedPreCommit, SignedPreVote, SignedProposal, VoteType, Weight,
};
use crate::{Adapter, Address, Blk, Hash, Height, OverlordError, OverlordResult, Round, St};

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Display)]
pub enum Capsule<B: Blk> {
    #[display(fmt = "signed_proposal: {}", _0)]
    SignedProposal(SignedProposal<B>),
    #[display(fmt = "signed_pre_vote: {}", _0)]
    SignedPreVote(SignedPreVote),
    #[display(fmt = "signed_pre_commit: {}", _0)]
    SignedPreCommit(SignedPreCommit),
    #[display(fmt = "signed_choke: {}", _0)]
    SignedChoke(SignedChoke),
    #[display(fmt = "pre_vote_qc: {}", _0)]
    PreVoteQC(PreVoteQC),
    #[display(fmt = "pre_commit_qc: {}", _0)]
    PreCommitQC(PreCommitQC),
    #[display(fmt = "choke_qc: {}", _0)]
    ChokeQC(ChokeQC),
}

impl_from!(
    Capsule,
    [
        tag SignedProposal,
        SignedPreVote,
        SignedPreCommit,
        SignedChoke,
        PreVoteQC,
        PreCommitQC,
        ChokeQC
    ]
);

#[derive(Default)]
pub struct Cabinet<B: Blk>(BTreeMap<Height, Drawer<B>>);

impl<B: Blk> Cabinet<B> {
    pub fn pop(&mut self, height: Height) -> Option<Vec<Grid<B>>> {
        self.0.remove(&height).map_or_else(
            || None,
            |drawer| Some(drawer.grids.values().cloned().collect()),
        )
    }

    pub fn next_height(&mut self, new_height: Height) {
        self.0 = self.0.split_off(&new_height);
    }

    pub fn handle_commit<A: Adapter<B, S>, S: St>(
        &mut self,
        next_height: Height,
        next_auth: &AuthManage<A, B, S>,
    ) {
        if let Some(grids) = self.pop(next_height) {
            for grid in grids {
                if let Some(sp) = grid.get_signed_proposal() {
                    if next_auth.verify_signed_proposal(sp).is_ok() {
                        let _ =
                            self.insert(sp.proposal.height, sp.proposal.round, sp.clone().into());
                    }
                }
                if let Some(qc) = grid.get_pre_vote_qc() {
                    if next_auth.verify_pre_vote_qc(&qc).is_ok() {
                        let _ = self.insert(qc.vote.height, qc.vote.round, qc.into());
                    }
                }
                if let Some(qc) = grid.get_pre_commit_qc() {
                    if next_auth.verify_pre_commit_qc(&qc).is_ok() {
                        let _ = self.insert(qc.vote.height, qc.vote.round, qc.into());
                    }
                }
                for sv in grid.get_signed_pre_votes() {
                    if next_auth.verify_signed_pre_vote(&sv).is_ok() {
                        let _ = self.insert(sv.vote.height, sv.vote.round, sv.into());
                    }
                }
                for sv in grid.get_signed_pre_commits() {
                    if next_auth.verify_signed_pre_commit(&sv).is_ok() {
                        let _ = self.insert(sv.vote.height, sv.vote.round, sv.into());
                    }
                }
                for sc in grid.get_signed_chokes() {
                    if next_auth.verify_signed_choke(&sc).is_ok() {
                        let _ = self.insert(sc.choke.height, sc.choke.round, sc.into());
                    }
                }
            }
        }
        self.next_height(next_height);
    }

    // Return max vote_weight when insert signed_pre_vote/signed_pre_commit/signed_choke
    pub fn insert(
        &mut self,
        height: Height,
        round: Round,
        data: Capsule<B>,
    ) -> OverlordResult<Option<CumWeight>> {
        self.0
            .entry(height)
            .or_insert_with(Drawer::default)
            .insert(round, data)
    }

    pub fn insert_full_block(&mut self, fetch: FetchedFullBlock) {
        self.0
            .entry(fetch.height)
            .or_insert_with(Drawer::default)
            .insert_full_block(fetch.block_hash, fetch.full_block)
    }

    pub fn get_block(&self, height: Height, hash: &Hash) -> Option<&B> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_block(hash))
    }

    pub fn get_full_block(&self, height: Height, hash: &Hash) -> Option<&Bytes> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_full_block(hash))
    }

    pub fn get_signed_proposal(&self, height: Height, round: Round) -> Option<&SignedProposal<B>> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_signed_proposal(round))
    }

    pub fn get_signed_pre_votes_by_hash(
        &self,
        height: Height,
        round: Round,
        block_hash: &Hash,
    ) -> Option<Vec<SignedPreVote>> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_signed_pre_votes_by_hash(round, block_hash))
    }

    pub fn get_signed_pre_commits_by_hash(
        &self,
        height: Height,
        round: Round,
        block_hash: &Hash,
    ) -> Option<Vec<SignedPreCommit>> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_signed_pre_commits_by_hash(round, block_hash))
    }

    pub fn get_signed_chokes(&self, height: Height, round: Round) -> Option<Vec<SignedChoke>> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_signed_chokes(round))
    }

    pub fn get_pre_vote_qc(&self, height: Height, round: Round) -> Option<PreVoteQC> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_pre_vote_qc(round))
    }

    pub fn get_pre_commit_qc(&self, height: Height, round: Round) -> Option<PreCommitQC> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_pre_commit_qc(round))
    }

    pub fn get_choke_qc(&self, height: Height, round: Round) -> Option<ChokeQC> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_choke_qc(round))
    }
}

#[derive(Default)]
struct Drawer<B: Blk> {
    grids: HashMap<Round, Grid<B>>,
    blocks: HashMap<Hash, B>,
    full_blocks: HashMap<Hash, Bytes>,
    pre_vote_max_vote_weight: CumWeight,
    pre_commit_max_vote_weight: CumWeight,
    choke_max_vote_weight: CumWeight,
}

impl<B: Blk> Drawer<B> {
    fn insert(&mut self, round: Round, data: Capsule<B>) -> OverlordResult<Option<CumWeight>> {
        if let Capsule::SignedProposal(signed_proposal) = &data {
            self.insert_block(&signed_proposal.proposal);
        }

        let opt = self
            .grids
            .entry(round)
            .or_insert_with(Grid::default)
            .insert(data)?;

        if let Some(cum_weight) = opt {
            return match cum_weight.vote_type {
                VoteType::PreVote => {
                    update_max_vote_weight(&mut self.pre_vote_max_vote_weight, cum_weight);
                    Ok(Some(self.pre_vote_max_vote_weight.clone()))
                }
                VoteType::PreCommit => {
                    update_max_vote_weight(&mut self.pre_commit_max_vote_weight, cum_weight);
                    Ok(Some(self.pre_commit_max_vote_weight.clone()))
                }
                VoteType::Choke => {
                    update_max_vote_weight(&mut self.choke_max_vote_weight, cum_weight);
                    Ok(Some(self.choke_max_vote_weight.clone()))
                }
            };
        }
        Ok(None)
    }

    fn insert_block(&mut self, proposal: &Proposal<B>) {
        self.blocks
            .entry(proposal.block_hash.clone())
            .or_insert_with(|| proposal.block.clone());
    }

    fn insert_full_block(&mut self, hash: Hash, full_block: Bytes) {
        self.full_blocks.entry(hash).or_insert_with(|| full_block);
    }

    fn get_block(&self, hash: &Hash) -> Option<&B> {
        self.blocks.get(hash)
    }

    fn get_full_block(&self, hash: &Hash) -> Option<&Bytes> {
        self.full_blocks.get(hash)
    }

    fn get_signed_proposal(&self, round: Round) -> Option<&SignedProposal<B>> {
        self.grids
            .get(&round)
            .and_then(|grid| grid.get_signed_proposal())
    }

    fn get_signed_pre_votes_by_hash(
        &self,
        round: Round,
        block_hash: &Hash,
    ) -> Option<Vec<SignedPreVote>> {
        self.grids
            .get(&round)
            .and_then(|grid| grid.get_signed_pre_votes_by_hash(block_hash))
    }

    fn get_signed_pre_commits_by_hash(
        &self,
        round: Round,
        block_hash: &Hash,
    ) -> Option<Vec<SignedPreCommit>> {
        self.grids
            .get(&round)
            .and_then(|grid| grid.get_signed_pre_commits_by_hash(block_hash))
    }

    fn get_signed_chokes(&self, round: Round) -> Option<Vec<SignedChoke>> {
        self.grids.get(&round).map(|grid| grid.get_signed_chokes())
    }

    fn get_pre_vote_qc(&self, round: Round) -> Option<PreVoteQC> {
        self.grids
            .get(&round)
            .and_then(|grid| grid.get_pre_vote_qc())
    }

    fn get_pre_commit_qc(&self, round: Round) -> Option<PreCommitQC> {
        self.grids
            .get(&round)
            .and_then(|grid| grid.get_pre_commit_qc())
    }

    fn get_choke_qc(&self, round: Round) -> Option<ChokeQC> {
        self.grids.get(&round).and_then(|grid| grid.get_choke_qc())
    }
}

#[derive(Clone, Default)]
pub struct Grid<B: Blk> {
    signed_proposal: Option<SignedProposal<B>>,

    signed_pre_votes:         HashMap<Address, SignedPreVote>,
    pre_vote_sets:            HashMap<Hash, Vec<SignedPreVote>>,
    pre_vote_vote_weights:    HashMap<Hash, Weight>,
    pre_vote_max_vote_weight: CumWeight,

    signed_pre_commits:         HashMap<Address, SignedPreCommit>,
    pre_commit_sets:            HashMap<Hash, Vec<SignedPreCommit>>,
    pre_commit_vote_weights:    HashMap<Hash, Weight>,
    pre_commit_max_vote_weight: CumWeight,

    signed_chokes:     HashMap<Address, SignedChoke>,
    choke_vote_weight: CumWeight,

    pre_vote_qc:   Option<PreVoteQC>,
    pre_commit_qc: Option<PreCommitQC>,
    choke_qc:      Option<ChokeQC>,
}

impl<B: Blk> Grid<B> {
    pub fn get_signed_proposal(&self) -> Option<&SignedProposal<B>> {
        self.signed_proposal.as_ref()
    }

    pub fn get_signed_pre_votes(&self) -> Vec<SignedPreVote> {
        self.signed_pre_votes
            .iter()
            .map(|(_, signed_pre_vote)| signed_pre_vote.clone())
            .collect()
    }

    pub fn get_signed_pre_commits(&self) -> Vec<SignedPreCommit> {
        self.signed_pre_commits
            .iter()
            .map(|(_, signed_pre_commit)| signed_pre_commit.clone())
            .collect()
    }

    pub fn get_signed_chokes(&self) -> Vec<SignedChoke> {
        self.signed_chokes
            .iter()
            .map(|(_, signed_choke)| signed_choke.clone())
            .collect()
    }

    pub fn get_pre_vote_qc(&self) -> Option<PreVoteQC> {
        self.pre_vote_qc.clone()
    }

    pub fn get_pre_commit_qc(&self) -> Option<PreCommitQC> {
        self.pre_commit_qc.clone()
    }

    pub fn get_choke_qc(&self) -> Option<ChokeQC> {
        self.choke_qc.clone()
    }

    fn get_signed_pre_votes_by_hash(&self, block_hash: &Hash) -> Option<Vec<SignedPreVote>> {
        self.pre_vote_sets.get(block_hash).cloned()
    }

    fn get_signed_pre_commits_by_hash(&self, block_hash: &Hash) -> Option<Vec<SignedPreCommit>> {
        self.pre_commit_sets.get(block_hash).cloned()
    }

    fn insert(&mut self, data: Capsule<B>) -> OverlordResult<Option<CumWeight>> {
        match data {
            Capsule::SignedProposal(signed_proposal) => {
                self.insert_signed_proposal(signed_proposal)
            }
            Capsule::SignedPreVote(signed_pre_vote) => self.insert_signed_pre_vote(signed_pre_vote),
            Capsule::SignedPreCommit(signed_pre_commit) => {
                self.insert_signed_pre_commit(signed_pre_commit)
            }
            Capsule::SignedChoke(signed_choke) => self.insert_signed_choke(signed_choke),
            Capsule::PreVoteQC(pre_vote_qc) => self.insert_pre_vote_qc(pre_vote_qc),
            Capsule::PreCommitQC(pre_commit_qc) => self.insert_pre_commit_qc(pre_commit_qc),
            Capsule::ChokeQC(choke_qc) => self.insert_choke_qc(choke_qc),
        }
    }

    fn insert_signed_proposal(
        &mut self,
        signed_proposal: SignedProposal<B>,
    ) -> OverlordResult<Option<CumWeight>> {
        check_exist(self.signed_proposal.as_ref(), &signed_proposal)?;

        self.signed_proposal = Some(signed_proposal);
        Ok(None)
    }

    fn insert_signed_pre_vote(
        &mut self,
        signed_pre_vote: SignedPreVote,
    ) -> OverlordResult<Option<CumWeight>> {
        let voter = signed_pre_vote.voter.clone();
        check_exist(self.signed_pre_votes.get(&voter), &signed_pre_vote)?;

        let hash = signed_pre_vote.vote.block_hash.clone();
        let cum_weight = update_vote_weight_map(
            &mut self.pre_vote_vote_weights,
            &hash,
            signed_pre_vote.vote_weight,
        );
        let cum_weight = CumWeight::new(
            cum_weight,
            VoteType::PreVote,
            signed_pre_vote.vote.round,
            Some(hash.clone()),
        );
        update_max_vote_weight(&mut self.pre_vote_max_vote_weight, cum_weight);

        self.signed_pre_votes.insert(voter, signed_pre_vote.clone());
        self.pre_vote_sets
            .entry(hash)
            .or_insert_with(Vec::new)
            .push(signed_pre_vote);

        Ok(Some(self.pre_vote_max_vote_weight.clone()))
    }

    fn insert_signed_pre_commit(
        &mut self,
        signed_pre_commit: SignedPreCommit,
    ) -> OverlordResult<Option<CumWeight>> {
        let voter = signed_pre_commit.voter.clone();
        check_exist(self.signed_pre_commits.get(&voter), &signed_pre_commit)?;

        let hash = signed_pre_commit.vote.block_hash.clone();
        let cum_weight = update_vote_weight_map(
            &mut self.pre_commit_vote_weights,
            &hash,
            signed_pre_commit.vote_weight,
        );
        let cum_weight = CumWeight::new(
            cum_weight,
            VoteType::PreCommit,
            signed_pre_commit.vote.round,
            Some(hash.clone()),
        );
        update_max_vote_weight(&mut self.pre_commit_max_vote_weight, cum_weight);

        self.signed_pre_commits
            .insert(voter, signed_pre_commit.clone());
        self.pre_commit_sets
            .entry(hash)
            .or_insert_with(Vec::new)
            .push(signed_pre_commit);

        Ok(Some(self.pre_commit_max_vote_weight.clone()))
    }

    fn insert_signed_choke(
        &mut self,
        signed_choke: SignedChoke,
    ) -> OverlordResult<Option<CumWeight>> {
        let voter = signed_choke.voter.clone();
        check_exist(self.signed_chokes.get(&voter), &signed_choke)?;

        let vote_weight = signed_choke.vote_weight;
        let cum_weight = self.choke_vote_weight.cum_weight + vote_weight;
        let cum_weight =
            CumWeight::new(cum_weight, VoteType::Choke, signed_choke.choke.round, None);
        update_max_vote_weight(&mut self.choke_vote_weight, cum_weight);

        self.signed_chokes.insert(voter, signed_choke);

        Ok(Some(self.choke_vote_weight.clone()))
    }

    fn insert_pre_vote_qc(&mut self, pre_vote_qc: PreVoteQC) -> OverlordResult<Option<CumWeight>> {
        check_exist(self.pre_vote_qc.as_ref(), &pre_vote_qc)?;
        self.pre_vote_qc = Some(pre_vote_qc);
        Ok(None)
    }

    fn insert_pre_commit_qc(
        &mut self,
        pre_commit_qc: PreCommitQC,
    ) -> OverlordResult<Option<CumWeight>> {
        check_exist(self.pre_commit_qc.as_ref(), &pre_commit_qc)?;
        self.pre_commit_qc = Some(pre_commit_qc);
        Ok(None)
    }

    fn insert_choke_qc(&mut self, choke_qc: ChokeQC) -> OverlordResult<Option<CumWeight>> {
        check_exist(self.choke_qc.as_ref(), &choke_qc)?;
        self.choke_qc = Some(choke_qc);
        Ok(None)
    }
}

fn check_exist<T: PartialEq + Eq>(opt: Option<&T>, check_data: &T) -> OverlordResult<()> {
    if let Some(exist_data) = opt {
        if exist_data == check_data {
            return Err(OverlordError::net_msg_exist());
        }
        return Err(OverlordError::byz_mul_version());
    }
    Ok(())
}

fn update_vote_weight_map(
    vote_weight_map: &mut HashMap<Address, Weight>,
    hash: &Hash,
    vote_weight: Weight,
) -> Weight {
    let mut cum_weight = vote_weight;
    if let Some(vote_weight) = vote_weight_map.get(hash) {
        cum_weight += *vote_weight;
    }
    vote_weight_map.insert(hash.clone(), cum_weight);
    cum_weight
}

fn update_max_vote_weight(max_weight: &mut CumWeight, cum_weight: CumWeight) {
    if max_weight.cum_weight < cum_weight.cum_weight {
        *max_weight = cum_weight;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::Proof;
    use crate::{Crypto, DefaultCrypto};
    use bytes::Bytes;
    use serde::{Deserialize, Serialize};
    use std::error::Error;

    #[test]
    fn test_cabinet() {
        let mut cabinet = Cabinet::<Block>::default();

        let mut signed_pre_vote = SignedPreVote::default();
        check_vote(
            &mut cabinet,
            &mut signed_pre_vote,
            VoteType::PreVote,
            Some(Hash::default()),
        );

        let mut signed_pre_commit = SignedPreCommit::default();
        check_vote(
            &mut cabinet,
            &mut signed_pre_commit,
            VoteType::PreCommit,
            Some(Hash::default()),
        );

        let mut signed_choke = SignedChoke::default();
        check_vote(&mut cabinet, &mut signed_choke, VoteType::Choke, None);

        let signed_proposal = SignedProposal::default();
        check_insert(&mut cabinet, &signed_proposal);
        assert_eq!(cabinet.get_signed_proposal(0, 0).unwrap(), &signed_proposal);

        let pre_vote_qc = PreVoteQC::default();
        check_insert(&mut cabinet, &pre_vote_qc);
        assert_eq!(cabinet.get_pre_vote_qc(0, 0).unwrap(), pre_vote_qc);

        let pre_commit_qc = PreCommitQC::default();
        check_insert(&mut cabinet, &pre_commit_qc);
        assert_eq!(cabinet.get_pre_commit_qc(0, 0).unwrap(), pre_commit_qc);

        let choke_qc = ChokeQC::default();
        check_insert(&mut cabinet, &choke_qc);
        assert_eq!(cabinet.get_choke_qc(0, 0).unwrap(), choke_qc);

        // test pop
        assert!(!cabinet.0.is_empty());
        let grids = cabinet.pop(0).unwrap();
        assert!(cabinet.0.is_empty());
        assert_eq!(grids.len(), 2);

        // test remove
        cabinet.insert(98, 0, choke_qc.clone().into()).unwrap();
        cabinet.insert(99, 0, choke_qc.clone().into()).unwrap();
        cabinet.insert(100, 0, choke_qc.into()).unwrap();
        cabinet.next_height(100);
        assert_eq!(cabinet.0.len(), 1);
    }

    fn check_insert<T: Clone + Into<Capsule<Block>>>(grid: &mut Cabinet<Block>, data: &T) {
        assert_eq!(grid.insert(0, 0, data.clone().into()).unwrap(), None);
    }

    fn check_vote<V: Vote>(
        grid: &mut Cabinet<Block>,
        vote: &mut V,
        vote_type: VoteType,
        opt_hash: Option<Hash>,
    ) {
        vote.set(0, 10, Bytes::from("wcc"));
        assert_eq!(
            grid.insert(0, 0, vote.clone().into()).unwrap(),
            Some(CumWeight::new(10, vote_type.clone(), 0, opt_hash.clone()))
        );

        assert!(grid.insert(0, 0, vote.clone().into()).is_err());
        vote.set(0, 12, Bytes::from("wcc"));
        assert!(grid.insert(0, 0, vote.clone().into()).is_err());
        vote.set(0, 4, Bytes::from("zyc"));
        assert_eq!(
            grid.insert(0, 0, vote.clone().into()).unwrap(),
            Some(CumWeight::new(14, vote_type.clone(), 0, opt_hash.clone()))
        );
        vote.set(1, 21, Bytes::from("yjy"));
        assert_eq!(
            grid.insert(0, 1, vote.clone().into()).unwrap(),
            Some(CumWeight::new(21, vote_type.clone(), 1, opt_hash.clone()))
        );
        vote.set(0, 5, Bytes::from("zy"));
        assert_eq!(
            grid.insert(0, 0, vote.clone().into()).unwrap(),
            Some(CumWeight::new(21, vote_type, 1, opt_hash))
        );
    }

    trait Vote: Sized + Clone + Default + Into<Capsule<Block>> {
        fn set(&mut self, round: Round, weight: Weight, voter: Address);
    }

    impl Vote for SignedPreVote {
        fn set(&mut self, round: Round, weight: Weight, voter: Address) {
            self.vote.round = round;
            self.vote_weight = weight;
            self.voter = voter;
        }
    }

    impl Vote for SignedPreCommit {
        fn set(&mut self, round: Round, weight: Weight, voter: Address) {
            self.vote.round = round;
            self.vote_weight = weight;
            self.voter = voter;
        }
    }

    impl Vote for SignedChoke {
        fn set(&mut self, round: Round, weight: Weight, voter: Address) {
            self.choke.round = round;
            self.vote_weight = weight;
            self.voter = voter;
        }
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct Block {
        pub pre_hash:    Hash,
        pub height:      Height,
        pub exec_height: Height,
        pub pre_proof:   Proof,
    }

    impl Block {
        fn genesis_block() -> Self {
            Block::default()
        }
    }

    impl Blk for Block {
        fn fixed_encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
            Ok(bincode::serialize(self).map(Bytes::from).unwrap())
        }

        fn fixed_decode(data: &Bytes) -> Result<Self, Box<dyn Error + Send>> {
            Ok(bincode::deserialize(data.as_ref()).unwrap())
        }

        fn get_block_hash(&self) -> Hash {
            DefaultCrypto::hash(&self.fixed_encode().unwrap())
        }

        fn get_pre_hash(&self) -> Hash {
            self.pre_hash.clone()
        }

        fn get_height(&self) -> Height {
            self.height
        }

        fn get_exec_height(&self) -> Height {
            self.exec_height
        }

        fn get_proof(&self) -> Proof {
            self.pre_proof.clone()
        }
    }
}
