#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::error::Error;

use derive_more::Display;

use crate::types::{
    ChokeQC, PreCommitQC, PreVoteQC, SignedChoke, SignedPreCommit, SignedPreVote, SignedProposal,
    Weight,
};
use crate::{Address, Blk, Hash, Height, Round};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Display)]
pub enum CollectData<B: Blk> {
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

impl<B: Blk> From<SignedProposal<B>> for CollectData<B> {
    fn from(data: SignedProposal<B>) -> Self {
        CollectData::SignedProposal(data)
    }
}

macro_rules! impl_from_for_collect_data {
    ($($struct: ident),+) => {
        $(
            impl<B:Blk> From<$struct> for CollectData<B> {
                fn from(data: $struct) -> Self {
                    CollectData::$struct(data)
                }
            }
        )+
    }
}

impl_from_for_collect_data!(
    SignedPreVote,
    SignedPreCommit,
    SignedChoke,
    PreVoteQC,
    PreCommitQC,
    ChokeQC
);

#[derive(Default)]
pub struct HeightCollector<B: Blk>(BTreeMap<Height, RoundCollector<B>>);

impl<B: Blk> HeightCollector<B> {
    pub fn insert(
        &mut self,
        height: Height,
        round: Round,
        data: CollectData<B>,
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        self.0
            .entry(height)
            .or_insert_with(RoundCollector::default)
            .insert(round, data)
    }

    pub fn pop(&mut self, height: Height) -> Option<Vec<Collector<B>>> {
        self.0.remove(&height).map_or_else(
            || None,
            |round_collector| Some(round_collector.collectors.values().cloned().collect()),
        )
    }

    pub fn remove_below(&mut self, height: Height) {
        self.0.split_off(&height);
    }

    pub fn get_signed_proposal(&self, height: Height, round: Round) -> Option<SignedProposal<B>> {
        self.0
            .get(&height)
            .and_then(|round_collector| round_collector.get_signed_proposal(round))
    }

    pub fn get_signed_pre_votes_by_hash(
        &self,
        height: Height,
        round: Round,
        block_hash: &Hash,
    ) -> Option<Vec<SignedPreVote>> {
        self.0.get(&height).and_then(|round_collector| {
            round_collector.get_signed_pre_votes_by_hash(round, block_hash)
        })
    }

    pub fn get_signed_pre_commits_by_hash(
        &self,
        height: Height,
        round: Round,
        block_hash: &Hash,
    ) -> Option<Vec<SignedPreCommit>> {
        self.0.get(&height).and_then(|round_collector| {
            round_collector.get_signed_pre_commits_by_hash(round, block_hash)
        })
    }

    pub fn get_signed_chokes(&self, height: Height, round: Round) -> Option<Vec<SignedChoke>> {
        self.0
            .get(&height)
            .and_then(|round_collector| round_collector.get_signed_chokes(round))
    }

    pub fn get_pre_vote_qc(&self, height: Height, round: Round) -> Option<PreVoteQC> {
        self.0
            .get(&height)
            .and_then(|round_collector| round_collector.get_pre_vote_qc(round))
    }

    pub fn get_pre_commit_qc(&self, height: Height, round: Round) -> Option<PreCommitQC> {
        self.0
            .get(&height)
            .and_then(|round_collector| round_collector.get_pre_commit_qc(round))
    }

    pub fn get_choke_qc(&self, height: Height, round: Round) -> Option<ChokeQC> {
        self.0
            .get(&height)
            .and_then(|round_collector| round_collector.get_choke_qc(round))
    }
}

#[derive(Default)]
struct RoundCollector<B: Blk> {
    collectors:                 HashMap<Round, Collector<B>>,
    pre_vote_max_vote_weight:   CumulativeWeight,
    pre_commit_max_vote_weight: CumulativeWeight,
    choke_max_vote_weight:      CumulativeWeight,
}

impl<B: Blk> RoundCollector<B> {
    fn insert(
        &mut self,
        round: Round,
        data: CollectData<B>,
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        let opt = self
            .collectors
            .entry(round)
            .or_insert_with(Collector::default)
            .insert(data)?;
        if let Some(cumulative_weight) = opt {
            return match cumulative_weight.vote_type {
                VoteType::PreVote => {
                    update_max_vote_weight(&mut self.pre_vote_max_vote_weight, cumulative_weight);
                    Ok(Some(self.pre_vote_max_vote_weight.clone()))
                }
                VoteType::PreCommit => {
                    update_max_vote_weight(&mut self.pre_commit_max_vote_weight, cumulative_weight);
                    Ok(Some(self.pre_commit_max_vote_weight.clone()))
                }
                VoteType::Choke => {
                    update_max_vote_weight(&mut self.choke_max_vote_weight, cumulative_weight);
                    Ok(Some(self.choke_max_vote_weight.clone()))
                }
            };
        }
        Ok(None)
    }

    fn get_signed_proposal(&self, round: Round) -> Option<SignedProposal<B>> {
        self.collectors
            .get(&round)
            .and_then(|collector| collector.get_signed_proposal())
    }

    fn get_signed_pre_votes_by_hash(
        &self,
        round: Round,
        block_hash: &Hash,
    ) -> Option<Vec<SignedPreVote>> {
        self.collectors
            .get(&round)
            .and_then(|collector| collector.get_signed_pre_votes_by_hash(block_hash))
    }

    fn get_signed_pre_commits_by_hash(
        &self,
        round: Round,
        block_hash: &Hash,
    ) -> Option<Vec<SignedPreCommit>> {
        self.collectors
            .get(&round)
            .and_then(|collector| collector.get_signed_pre_commits_by_hash(block_hash))
    }

    fn get_signed_chokes(&self, round: Round) -> Option<Vec<SignedChoke>> {
        self.collectors
            .get(&round)
            .map(|collector| collector.get_signed_chokes())
    }

    fn get_pre_vote_qc(&self, round: Round) -> Option<PreVoteQC> {
        self.collectors
            .get(&round)
            .and_then(|collector| collector.get_pre_vote_qc())
    }

    fn get_pre_commit_qc(&self, round: Round) -> Option<PreCommitQC> {
        self.collectors
            .get(&round)
            .and_then(|collector| collector.get_pre_commit_qc())
    }

    fn get_choke_qc(&self, round: Round) -> Option<ChokeQC> {
        self.collectors
            .get(&round)
            .and_then(|collector| collector.get_choke_qc())
    }
}

#[derive(Clone)]
pub enum VoteType {
    PreVote,
    PreCommit,
    Choke,
}

impl Default for VoteType {
    fn default() -> Self {
        VoteType::PreVote
    }
}

#[derive(Clone, Default)]
pub struct CumulativeWeight {
    cumulative_weight: Weight,
    vote_type:         VoteType,
    round:             Round,
    block_hash:        Option<Hash>,
}

impl CumulativeWeight {
    fn new(
        cumulative_weight: Weight,
        vote_type: VoteType,
        round: Round,
        block_hash: Option<Hash>,
    ) -> Self {
        CumulativeWeight {
            cumulative_weight,
            vote_type,
            round,
            block_hash,
        }
    }
}

#[derive(Clone, Default)]
pub struct Collector<B: Blk> {
    signed_proposal: Option<SignedProposal<B>>,

    signed_pre_votes:         HashMap<Address, SignedPreVote>,
    pre_vote_sets:            HashMap<Hash, Vec<SignedPreVote>>,
    pre_vote_vote_weights:    HashMap<Hash, Weight>,
    pre_vote_max_vote_weight: CumulativeWeight,

    signed_pre_commits:         HashMap<Address, SignedPreCommit>,
    pre_commit_sets:            HashMap<Hash, Vec<SignedPreCommit>>,
    pre_commit_vote_weights:    HashMap<Hash, Weight>,
    pre_commit_max_vote_weight: CumulativeWeight,

    signed_chokes:     HashMap<Address, SignedChoke>,
    choke_vote_weight: CumulativeWeight,

    pre_vote_qc:   Option<PreVoteQC>,
    pre_commit_qc: Option<PreCommitQC>,
    choke_qc:      Option<ChokeQC>,
}

impl<B: Blk> Collector<B> {
    pub fn get_signed_proposal(&self) -> Option<SignedProposal<B>> {
        self.signed_proposal.clone()
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

    fn insert(
        &mut self,
        data: CollectData<B>,
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        match data {
            CollectData::SignedProposal(signed_proposal) => {
                self.insert_signed_proposal(signed_proposal)
            }
            CollectData::SignedPreVote(signed_pre_vote) => {
                self.insert_signed_pre_vote(signed_pre_vote)
            }
            CollectData::SignedPreCommit(signed_pre_commit) => {
                self.insert_signed_pre_commit(signed_pre_commit)
            }
            CollectData::SignedChoke(signed_choke) => self.insert_signed_choke(signed_choke),
            CollectData::PreVoteQC(pre_vote_qc) => self.insert_pre_vote_qc(pre_vote_qc),
            CollectData::PreCommitQC(pre_commit_qc) => self.insert_pre_commit_qc(pre_commit_qc),
            CollectData::ChokeQC(choke_qc) => self.insert_choke_qc(choke_qc),
        }
    }

    fn insert_signed_proposal(
        &mut self,
        signed_proposal: SignedProposal<B>,
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        check_exist(self.signed_proposal.as_ref(), &signed_proposal)?;

        self.signed_proposal = Some(signed_proposal);
        Ok(None)
    }

    fn insert_signed_pre_vote(
        &mut self,
        signed_pre_vote: SignedPreVote,
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        let voter = signed_pre_vote.voter.clone();
        check_exist(self.signed_pre_votes.get(&voter), &signed_pre_vote)?;

        let hash = signed_pre_vote.vote.block_hash.clone();
        let cumulative_weight = update_vote_weight_map(
            &mut self.pre_vote_vote_weights,
            &hash,
            signed_pre_vote.vote_weight,
        );
        let cumulative_weight = CumulativeWeight::new(
            cumulative_weight,
            VoteType::PreVote,
            signed_pre_vote.vote.round,
            Some(hash.clone()),
        );
        update_max_vote_weight(&mut self.pre_vote_max_vote_weight, cumulative_weight);

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
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        let voter = signed_pre_commit.voter.clone();
        check_exist(self.signed_pre_commits.get(&voter), &signed_pre_commit)?;

        let hash = signed_pre_commit.vote.block_hash.clone();
        let cumulative_weight = update_vote_weight_map(
            &mut self.pre_commit_vote_weights,
            &hash,
            signed_pre_commit.vote_weight,
        );
        let cumulative_weight = CumulativeWeight::new(
            cumulative_weight,
            VoteType::PreCommit,
            signed_pre_commit.vote.round,
            Some(hash.clone()),
        );
        update_max_vote_weight(&mut self.pre_commit_max_vote_weight, cumulative_weight);

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
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        let signer = signed_choke.signer.clone();
        check_exist(self.signed_chokes.get(&signer), &signed_choke)?;

        let vote_weight = signed_choke.vote_weight;
        let cumulative_weight = self.choke_vote_weight.cumulative_weight + vote_weight;
        let cumulative_weight = CumulativeWeight::new(
            cumulative_weight,
            VoteType::Choke,
            signed_choke.choke.round,
            None,
        );
        update_max_vote_weight(&mut self.choke_vote_weight, cumulative_weight);

        self.signed_chokes.insert(signer, signed_choke);

        Ok(Some(self.choke_vote_weight.clone()))
    }

    fn insert_pre_vote_qc(
        &mut self,
        pre_vote_qc: PreVoteQC,
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        check_exist(self.pre_vote_qc.as_ref(), &pre_vote_qc)?;
        self.pre_vote_qc = Some(pre_vote_qc);
        Ok(None)
    }

    fn insert_pre_commit_qc(
        &mut self,
        pre_commit_qc: PreCommitQC,
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        check_exist(self.pre_commit_qc.as_ref(), &pre_commit_qc)?;
        self.pre_commit_qc = Some(pre_commit_qc);
        Ok(None)
    }

    fn insert_choke_qc(
        &mut self,
        choke_qc: ChokeQC,
    ) -> Result<Option<CumulativeWeight>, CollectionError<B>> {
        check_exist(self.choke_qc.as_ref(), &choke_qc)?;
        self.choke_qc = Some(choke_qc);
        Ok(None)
    }
}

fn check_exist<T: Into<CollectData<B>> + Clone + PartialEq + Eq, B: Blk>(
    opt: Option<&T>,
    check_data: &T,
) -> Result<(), CollectionError<B>> {
    if let Some(exist_data) = opt {
        if exist_data == check_data {
            return Err(CollectionError::AlreadyExists(check_data.clone().into()));
        }
        return Err(CollectionError::InsertDiff(
            exist_data.clone().into(),
            check_data.clone().into(),
        ));
    }
    Ok(())
}

fn update_vote_weight_map(
    vote_weight_map: &mut HashMap<Address, Weight>,
    hash: &Hash,
    vote_weight: Weight,
) -> Weight {
    let mut cumulative_weight = vote_weight;
    if let Some(vote_weight) = vote_weight_map.get(hash) {
        cumulative_weight += *vote_weight;
    }
    vote_weight_map.insert(hash.clone(), cumulative_weight);
    cumulative_weight
}

fn update_max_vote_weight(max_weight: &mut CumulativeWeight, cumulative_weight: CumulativeWeight) {
    if max_weight.cumulative_weight < cumulative_weight.cumulative_weight {
        *max_weight = cumulative_weight;
    }
}

#[allow(dead_code)]
#[derive(Debug, Display)]
pub enum CollectionError<B: Blk> {
    #[display(fmt = "collect_data: {} already exists", _0)]
    AlreadyExists(CollectData<B>),
    #[display(
        fmt = "try to insert a different collect_data: {}, while exist {}",
        _0,
        _1
    )]
    InsertDiff(CollectData<B>, CollectData<B>),
}

impl<B: Blk> Error for CollectionError<B> {}
