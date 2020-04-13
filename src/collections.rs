use std::collections::{BTreeMap, HashMap};
use std::error::Error;

use derive_more::Display;

use crate::types::{
    ChokeQC, PreCommitQC, PreVoteQC, SignedChoke, SignedPreCommit, SignedPreVote, SignedProposal,
    Weight,
};
use crate::{Address, Blk, Hash, Height, Round};

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
    ) -> Result<(), CollectionError<B>> {
        self.0
            .entry(height)
            .or_insert_with(RoundCollector::default)
            .insert(round, data)
    }

    // pub fn pop(&mut self, height: &Height) -> Option<Vec<Collector<B>>> {
    //     self.0.remove(height).map_or_else(
    //         || None,
    //         |round_collector| Some(round_collector.0.values().cloned().collect())
    //     )
    // }

    pub fn remove_below(&mut self, height: &Height) {
        self.0.split_off(height);
    }
}

#[derive(Default)]
struct RoundCollector<B: Blk>(HashMap<Round, Collector<B>>);

impl<B: Blk> RoundCollector<B> {
    fn insert(&mut self, round: Round, data: CollectData<B>) -> Result<(), CollectionError<B>> {
        self.0
            .entry(round)
            .or_insert_with(Collector::default)
            .insert(data)
    }

    fn get(&self, round: &Round) -> Option<&Collector<B>> {
        self.0.get(round)
    }
}

#[derive(Clone, Default)]
pub struct WeightOfHash {
    weight: Weight,
    hash:   Hash,
}

impl WeightOfHash {
    fn new(weight: Weight, hash: Hash) -> Self {
        WeightOfHash {
            weight,
            hash,
        }
    }
}

#[derive(Clone, Default)]
struct Collector<B: Blk> {
    signed_proposal: Option<SignedProposal<B>>,

    signed_pre_votes: HashMap<Address, SignedPreVote>,
    pre_vote_sets:   HashMap<Hash, Vec<SignedPreVote>>,
    pre_vote_weights:HashMap<Hash, Weight>,
    pre_vote_max_weight: WeightOfHash,

    signed_pre_commits: HashMap<Address, SignedPreCommit>,
    pre_commit_sets:        HashMap<Hash, Vec<SignedPreCommit>>,
    pre_commit_weights:HashMap<Hash, Weight>,
    pre_commit_max_weight: WeightOfHash,

    signed_chokes: HashMap<Address, SignedChoke>,
    choke_weight:   Weight,

    pre_vote_qc:   Option<PreVoteQC>,
    pre_commit_qc: Option<PreCommitQC>,
    choke_qc:      Option<ChokeQC>,
}

impl<B: Blk> Collector<B> {
    fn insert(&mut self, data: CollectData<B>) -> Result<(), CollectionError<B>> {
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
    ) -> Result<(), CollectionError<B>> {
        check_exist(self.signed_proposal.as_ref(), &signed_proposal)?;

        self.signed_proposal = Some(signed_proposal);
        Ok(())
    }

    fn insert_signed_pre_vote(
        &mut self,
        signed_pre_vote: SignedPreVote,
    ) -> Result<(), CollectionError<B>> {
        let voter = signed_pre_vote.voter.clone();
        check_exist(self.signed_pre_votes.get(&voter), &signed_pre_vote)?;

        let hash = signed_pre_vote.vote.block_hash.clone();
        let weight = signed_pre_vote.vote_weight;
        let new_weight = update_weight_map(&mut self.pre_vote_weights, &hash, weight);
        update_max_weight(&mut self.pre_vote_max_weight, hash.clone(), new_weight);

        self.signed_pre_votes.insert(voter, signed_pre_vote.clone());
        self.pre_vote_sets.entry(hash).or_insert_with(Vec::new).push(signed_pre_vote);

        Ok(())
    }

    fn insert_signed_pre_commit(
        &mut self,
        signed_pre_commit: SignedPreCommit,
    ) -> Result<(), CollectionError<B>> {
        let voter = signed_pre_commit.voter.clone();
        check_exist(self.signed_pre_commits.get(&voter), &signed_pre_commit)?;

        let hash = signed_pre_commit.vote.block_hash.clone();
        let weight = signed_pre_commit.vote_weight;
        let new_weight = update_weight_map(&mut self.pre_commit_weights, &hash, weight);
        update_max_weight(&mut self.pre_commit_max_weight, hash.clone(), new_weight);

        self.signed_pre_commits.insert(voter, signed_pre_commit.clone());
        self.pre_commit_sets.entry(hash).or_insert_with(Vec::new).push(signed_pre_commit);

        Ok(())
    }

    fn insert_signed_choke(
        &mut self,
        signed_choke: SignedChoke,
    ) -> Result<(), CollectionError<B>> {
        let signer = signed_choke.signer.clone();
        check_exist(self.signed_chokes.get(&signer), &signed_choke)?;

        let weight = signed_choke.vote_weight;
        self.choke_weight += weight;

        self.signed_chokes.insert(signer, signed_choke);

        Ok(())
    }

    fn insert_pre_vote_qc(&mut self, pre_vote_qc: PreVoteQC) -> Result<(), CollectionError<B>> {
        check_exist(self.pre_vote_qc.as_ref(), &pre_vote_qc)?;
        self.pre_vote_qc = Some(pre_vote_qc);
        Ok(())
    }

    fn insert_pre_commit_qc(
        &mut self,
        pre_commit_qc: PreCommitQC,
    ) -> Result<(), CollectionError<B>> {
        check_exist(self.pre_commit_qc.as_ref(), &pre_commit_qc)?;
        self.pre_commit_qc = Some(pre_commit_qc);
        Ok(())
    }

    fn insert_choke_qc(&mut self, choke_qc: ChokeQC) -> Result<(), CollectionError<B>> {
        check_exist(self.choke_qc.as_ref(), &choke_qc)?;
        self.choke_qc = Some(choke_qc);
        Ok(())
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

fn update_weight_map(weight_map: &mut HashMap<Address, Weight>, hash: &Hash, weight: Weight) -> Weight {
    let mut new_weight = weight;
    if let Some(weight) = weight_map.get(hash) {
        new_weight += *weight;
    }
    weight_map.insert(hash.clone(), new_weight);
    new_weight
}

fn update_max_weight(max_weight: &mut WeightOfHash, hash: Hash, new_weight: Weight) {
    if max_weight.weight < new_weight {
        *max_weight = WeightOfHash::new(new_weight, hash);
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
