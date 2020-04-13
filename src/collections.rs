use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;

use derive_more::Display;

use crate::{Address, Blk, Hash, Height, Round};
use crate::types::{SignedProposal, SignedPreVote, SignedPreCommit, PreVoteQC, Weight, PreCommitQC, SignedChoke, ChokeQC};
use std::process::exit;

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

macro_rules! impl_from_for_collect_data {
    (($struct: ident),+) => {
        (
            impl<B:Blk> From<$struct> for CollectData<B> {
                fn from(data: $struct) -> Self {
                    CollectData::$struct(data)
                }
            }
        )+
    }
}

pub struct HeightCollector<B: Blk>(BTreeMap<Height, RoundCollector<B>>);

impl<B: Blk> HeightCollector<B> {
    pub fn new() -> Self {
        HeightCollector(BTreeMap::new())
    }

    pub fn insert(&mut self, height: Height, round: Round, data: CollectData<B>) -> Result<(), CollectionError<B>> {
        self.0.entry(height).or_insert_with(RoundCollector::new).insert(round, data)
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

struct RoundCollector<B: Blk>(HashMap<Round, Collector<B>>);

impl<B: Blk> RoundCollector<B> {
    fn new() -> Self {
        RoundCollector(HashMap::new())
    }

    fn insert(&mut self, round: Round, data: CollectData<B>) -> Result<(), CollectionError<B>> {
        self.0.entry(round).or_insert_with(Collector::new).insert(data)
    }

    fn get(&self, round: &Round) -> Option<&Collector<B>> {
        self.0.get(round)
    }
}

#[derive(Clone)]
struct Collector<B: Blk> {
    signed_proposal: Option<SignedProposal<B>>,

    signed_pre_votes: HashMap<Address, SignedPreVote>,
    pre_vote_group: HashMap<Hash, (Weight, HashSet<SignedPreVote>)>,

    signed_pre_commit_votes: HashMap<Address, SignedPreCommit>,
    pre_commit_group: HashMap<Hash, (Weight, HashSet<SignedPreCommit>)>,

    pre_vote_qc: Option<PreVoteQC>,
    pre_commit_qc: Option<PreCommitQC>,
}

impl<B: Blk> Collector<B> {
    fn new() -> Self {
        Collector {
            signed_proposal: None,
            signed_pre_votes: HashMap::new(),
            pre_vote_group: HashMap::new(),
            signed_pre_commit_votes: HashMap::new(),
            pre_commit_group: HashMap::new(),
            pre_vote_qc: None,
            pre_commit_qc: None,
        }
    }

    fn insert(&mut self, data: CollectData<B>) -> Result<(), CollectionError<B>> {
        match data {
            CollectData::SignedProposal(signed_proposal) => self.insert_signed_proposal(signed_proposal),
            CollectData::SignedPreVote(signed_pre_vote) => self.insert_signed_pre_vote(signed_pre_vote),
            CollectData::SignedPreCommit(signed_pre_commit) => self.insert_signed_pre_commit(signed_pre_commit),
            CollectData::SignedChoke(signed_choke) => self.insert_signed_choke(signed_choke),
            CollectData::PreVoteQC(pre_vote_qc) => self.insert_pre_vote_qc(pre_vote_qc),
            CollectData::PreCommitQC(pre_commit_qc) => self.insert_pre_commit_qc(pre_commit_qc),
            CollectData::ChokeQC(choke_qc) => self.insert_choke_qc(choke_qc),
        }
    }

    fn insert_signed_proposal(&mut self, signed_proposal: SignedProposal<B>) -> Result<(), CollectionError<B>> {
        if let Some(exist_proposal) = self.signed_proposal.clone() {
            if exist_proposal == signed_proposal {
                return Err(CollectionError::AlreadyExists(CollectData::SignedProposal(signed_proposal)))
            } else {
                return Err(CollectionError::InsertDiff(CollectData::SignedProposal(signed_proposal), CollectData::SignedProposal(exist_proposal)))
            }
        } else {
            self.signed_proposal = Some(signed_proposal);
        }
        Ok(())
    }

    fn insert_signed_pre_vote(&mut self, signed_proposal: SignedPreVote) -> Result<(), CollectionError<B>> {
        Ok(())
    }

    fn insert_signed_pre_commit(&mut self, signed_proposal: SignedPreCommit) -> Result<(), CollectionError<B>> {
        Ok(())
    }

    fn insert_signed_choke(&mut self, signed_proposal: SignedChoke) -> Result<(), CollectionError<B>> {
        Ok(())
    }

    fn insert_pre_vote_qc(&mut self, signed_proposal: PreVoteQC) -> Result<(), CollectionError<B>> {
        Ok(())
    }

    fn insert_pre_commit_qc(&mut self, signed_proposal: PreCommitQC) -> Result<(), CollectionError<B>> {
        Ok(())
    }

    fn insert_choke_qc(&mut self, signed_proposal: ChokeQC) -> Result<(), CollectionError<B>> {
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Debug, Display)]
pub enum CollectionError<B: Blk> {
    #[display(fmt = "collect_data: {} already exists", _0)]
    AlreadyExists(CollectData<B>),
    #[display(fmt = "try to insert a different collect_data: {}, while exist {}", _0, _1)]
    InsertDiff(CollectData<B>, CollectData<B>),
}

impl<B: Blk> Error for CollectionError<B> {}