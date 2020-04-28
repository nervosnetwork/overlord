use std::collections::{BTreeMap, HashMap};

use bytes::Bytes;
use derive_more::Display;

use crate::types::{
    ChokeQC, CumWeight, FetchedFullBlock, PreCommitQC, PreVoteQC, Proposal, SignedChoke,
    SignedPreCommit, SignedPreVote, SignedProposal, VoteType, Weight,
};
use crate::{Address, Blk, Hash, Height, OverlordError, OverlordResult, Round};

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Display)]
pub enum Capsule<'a, B: Blk> {
    #[display(fmt = "signed_proposal: {}", _0)]
    SignedProposal(&'a SignedProposal<B>),
    #[display(fmt = "signed_pre_vote: {}", _0)]
    SignedPreVote(&'a SignedPreVote),
    #[display(fmt = "signed_pre_commit: {}", _0)]
    SignedPreCommit(&'a SignedPreCommit),
    #[display(fmt = "signed_choke: {}", _0)]
    SignedChoke(&'a SignedChoke),
    #[display(fmt = "pre_vote_qc: {}", _0)]
    PreVoteQC(&'a PreVoteQC),
    #[display(fmt = "pre_commit_qc: {}", _0)]
    PreCommitQC(&'a PreCommitQC),
    #[display(fmt = "choke_qc: {}", _0)]
    ChokeQC(&'a ChokeQC),
}

impl_from!(Capsule<'a, B: Blk>, [
    SignedProposal<B>,
    SignedPreVote,
    SignedPreCommit,
    SignedChoke,
    PreVoteQC,
    PreCommitQC,
    ChokeQC,
]);

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

    // Return max vote_weight when insert signed_pre_vote/signed_pre_commit/signed_choke
    pub fn insert(&mut self, height: Height, round: Round, data: Capsule<B>) -> OverlordResult<()> {
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

    pub fn take_signed_proposal(
        &mut self,
        height: Height,
        round: Round,
    ) -> Option<SignedProposal<B>> {
        self.0
            .get_mut(&height)
            .and_then(|drawer| drawer.take_signed_proposal(round))
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

    pub fn get_pre_vote_qc_by_hash(&self, height: Height, block_hash: &Hash) -> Option<PreVoteQC> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_pre_vote_qc_by_hash(block_hash))
    }

    pub fn get_pre_commit_qc_by_hash(
        &self,
        height: Height,
        block_hash: &Hash,
    ) -> Option<PreCommitQC> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_pre_commit_qc_by_hash(block_hash))
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

    pub fn get_pre_vote_max_vote_weight(&self, height: Height, round: Round) -> Option<CumWeight> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_pre_vote_max_vote_weight(round))
    }

    pub fn get_pre_commit_max_vote_weight(
        &self,
        height: Height,
        round: Round,
    ) -> Option<CumWeight> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_pre_commit_max_vote_weight(round))
    }

    pub fn get_choke_vote_weight(&self, height: Height, round: Round) -> Option<CumWeight> {
        self.0
            .get(&height)
            .and_then(|drawer| drawer.get_choke_vote_weight(round))
    }
}

#[derive(Default)]
struct Drawer<B: Blk> {
    grids:          HashMap<Round, Grid<B>>,
    blocks:         HashMap<Hash, B>,
    full_blocks:    HashMap<Hash, Bytes>,
    pre_vote_qcs:   HashMap<Hash, PreVoteQC>,
    pre_commit_qcs: HashMap<Hash, PreCommitQC>,
}

impl<B: Blk> Drawer<B> {
    fn insert(&mut self, round: Round, data: Capsule<B>) -> OverlordResult<()> {
        match data {
            Capsule::SignedProposal(sp) => self.insert_block(&sp.proposal),
            Capsule::PreVoteQC(qc) => self.insert_pre_vote_qc(qc),
            Capsule::PreCommitQC(qc) => self.insert_pre_commit_qc(qc),
            _ => {}
        }

        self.grids
            .entry(round)
            .or_insert_with(Grid::default)
            .insert(data)
    }

    fn insert_block(&mut self, proposal: &Proposal<B>) {
        self.blocks
            .entry(proposal.block_hash.clone())
            .or_insert_with(|| proposal.block.clone());
    }

    fn insert_full_block(&mut self, hash: Hash, full_block: Bytes) {
        self.full_blocks.entry(hash).or_insert_with(|| full_block);
    }

    fn insert_pre_vote_qc(&mut self, pre_vote_qc: &PreVoteQC) {
        let hash = pre_vote_qc.vote.block_hash.clone();
        if let Some(qc) = self.pre_vote_qcs.get(&hash) {
            if qc.vote.round < pre_vote_qc.vote.round {
                self.pre_vote_qcs.insert(hash, pre_vote_qc.clone());
            }
        } else {
            self.pre_vote_qcs.insert(hash, pre_vote_qc.clone());
        }
    }

    fn insert_pre_commit_qc(&mut self, pre_commit_qc: &PreCommitQC) {
        let hash = pre_commit_qc.vote.block_hash.clone();
        if let Some(qc) = self.pre_commit_qcs.get(&hash) {
            if qc.vote.round < pre_commit_qc.vote.round {
                self.pre_commit_qcs.insert(hash, pre_commit_qc.clone());
            }
        } else {
            self.pre_commit_qcs.insert(hash, pre_commit_qc.clone());
        }
    }

    fn get_block(&self, hash: &Hash) -> Option<&B> {
        self.blocks.get(hash)
    }

    fn get_full_block(&self, hash: &Hash) -> Option<&Bytes> {
        self.full_blocks.get(hash)
    }

    fn take_signed_proposal(&mut self, round: Round) -> Option<SignedProposal<B>> {
        self.grids
            .get_mut(&round)
            .and_then(|grid| grid.take_signed_proposal())
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

    fn get_pre_vote_qc_by_hash(&self, block_hash: &Hash) -> Option<PreVoteQC> {
        self.pre_vote_qcs.get(block_hash).cloned()
    }

    fn get_pre_commit_qc_by_hash(&self, block_hash: &Hash) -> Option<PreCommitQC> {
        self.pre_commit_qcs.get(block_hash).cloned()
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

    fn get_pre_vote_max_vote_weight(&self, round: Round) -> Option<CumWeight> {
        self.grids
            .get(&round)
            .map(|grid| grid.get_pre_vote_max_vote_weight())
    }

    fn get_pre_commit_max_vote_weight(&self, round: Round) -> Option<CumWeight> {
        self.grids
            .get(&round)
            .map(|grid| grid.get_pre_commit_max_vote_weight())
    }

    fn get_choke_vote_weight(&self, round: Round) -> Option<CumWeight> {
        self.grids
            .get(&round)
            .map(|grid| grid.get_choke_vote_weight())
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
    pub fn take_signed_proposal(&mut self) -> Option<SignedProposal<B>> {
        let signed_proposal = self.signed_proposal.clone();
        self.signed_proposal = None;
        signed_proposal
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

    fn get_pre_vote_max_vote_weight(&self) -> CumWeight {
        self.pre_vote_max_vote_weight.clone()
    }

    fn get_pre_commit_max_vote_weight(&self) -> CumWeight {
        self.pre_commit_max_vote_weight.clone()
    }

    fn get_choke_vote_weight(&self) -> CumWeight {
        self.choke_vote_weight.clone()
    }

    fn insert(&mut self, data: Capsule<B>) -> OverlordResult<()> {
        match data {
            Capsule::SignedProposal(signed_proposal) => {
                self.insert_signed_proposal(signed_proposal.clone())
            }
            Capsule::SignedPreVote(signed_pre_vote) => {
                self.insert_signed_pre_vote(signed_pre_vote.clone())
            }
            Capsule::SignedPreCommit(signed_pre_commit) => {
                self.insert_signed_pre_commit(signed_pre_commit.clone())
            }
            Capsule::SignedChoke(signed_choke) => self.insert_signed_choke(signed_choke.clone()),
            Capsule::PreVoteQC(pre_vote_qc) => self.insert_pre_vote_qc(pre_vote_qc.clone()),
            Capsule::PreCommitQC(pre_commit_qc) => self.insert_pre_commit_qc(pre_commit_qc.clone()),
            Capsule::ChokeQC(choke_qc) => self.insert_choke_qc(choke_qc.clone()),
        }
    }

    fn insert_signed_proposal(&mut self, signed_proposal: SignedProposal<B>) -> OverlordResult<()> {
        check_exist(self.signed_proposal.as_ref(), &signed_proposal)?;

        self.signed_proposal = Some(signed_proposal);
        Ok(())
    }

    fn insert_signed_pre_vote(&mut self, signed_pre_vote: SignedPreVote) -> OverlordResult<()> {
        let voter = signed_pre_vote.voter.clone();
        check_exist(self.signed_pre_votes.get(&voter), &signed_pre_vote)?;

        let hash = signed_pre_vote.vote.block_hash.clone();
        let cum_weight = update_vote_weight_map(
            &mut self.pre_vote_vote_weights,
            &hash,
            signed_pre_vote.vote_weight,
        );
        let cum_weight = CumWeight::new(cum_weight, VoteType::PreVote, Some(hash.clone()));
        update_max_vote_weight(&mut self.pre_vote_max_vote_weight, cum_weight);

        self.signed_pre_votes.insert(voter, signed_pre_vote.clone());
        self.pre_vote_sets
            .entry(hash)
            .or_insert_with(Vec::new)
            .push(signed_pre_vote);

        Ok(())
    }

    fn insert_signed_pre_commit(
        &mut self,
        signed_pre_commit: SignedPreCommit,
    ) -> OverlordResult<()> {
        let voter = signed_pre_commit.voter.clone();
        check_exist(self.signed_pre_commits.get(&voter), &signed_pre_commit)?;

        let hash = signed_pre_commit.vote.block_hash.clone();
        let cum_weight = update_vote_weight_map(
            &mut self.pre_commit_vote_weights,
            &hash,
            signed_pre_commit.vote_weight,
        );
        let cum_weight = CumWeight::new(cum_weight, VoteType::PreCommit, Some(hash.clone()));
        update_max_vote_weight(&mut self.pre_commit_max_vote_weight, cum_weight);

        self.signed_pre_commits
            .insert(voter, signed_pre_commit.clone());
        self.pre_commit_sets
            .entry(hash)
            .or_insert_with(Vec::new)
            .push(signed_pre_commit);

        Ok(())
    }

    fn insert_signed_choke(&mut self, signed_choke: SignedChoke) -> OverlordResult<()> {
        let voter = signed_choke.voter.clone();
        check_exist(self.signed_chokes.get(&voter), &signed_choke)?;

        let vote_weight = signed_choke.vote_weight;
        let cum_weight = self.choke_vote_weight.cum_weight + vote_weight;
        let cum_weight = CumWeight::new(cum_weight, VoteType::Choke, None);
        update_max_vote_weight(&mut self.choke_vote_weight, cum_weight);

        self.signed_chokes.insert(voter, signed_choke);

        Ok(())
    }

    fn insert_pre_vote_qc(&mut self, pre_vote_qc: PreVoteQC) -> OverlordResult<()> {
        check_exist(self.pre_vote_qc.as_ref(), &pre_vote_qc)?;
        self.pre_vote_qc = Some(pre_vote_qc);
        Ok(())
    }

    fn insert_pre_commit_qc(&mut self, qc: PreCommitQC) -> OverlordResult<()> {
        check_exist(self.pre_commit_qc.as_ref(), &qc)?;
        self.pre_commit_qc = Some(qc);
        Ok(())
    }

    fn insert_choke_qc(&mut self, qc: ChokeQC) -> OverlordResult<()> {
        check_exist(self.choke_qc.as_ref(), &qc)?;
        self.choke_qc = Some(qc);
        Ok(())
    }
}

fn check_exist<T: std::fmt::Display + PartialEq + Eq>(
    opt: Option<&T>,
    check_data: &T,
) -> OverlordResult<()> {
    if let Some(exist_data) = opt {
        if exist_data == check_data {
            return Err(OverlordError::debug_msg_exist());
        }
        return Err(OverlordError::byz_mul_version(format!(
            "insert.msg != exist.msg, {} != {}",
            check_data, exist_data
        )));
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
