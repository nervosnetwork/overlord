use std::collections::{BTreeMap, HashMap};

use crate::types::{
    Address, AggregatedVote, Hash, Signature, SignedProposal, SignedVote, VoteType,
};
use crate::{error::ConsensusError, Codec, ConsensusResult};

/// A struct to collect signed proposals in each epoch. It stores each epoch and the corresponding
/// signed proposals in a `BTreeMap`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalCollector<T: Codec>(BTreeMap<u64, ProposalRoundCollector<T>>);

impl<T> ProposalCollector<T>
where
    T: Codec,
{
    pub fn new() -> Self {
        ProposalCollector(BTreeMap::new())
    }

    pub fn insert(
        &mut self,
        epoch_id: u64,
        round: u64,
        proposal: SignedProposal<T>,
        hash: Hash,
    ) -> ConsensusResult<()> {
        self.0
            .entry(epoch_id)
            .or_insert_with(ProposalRoundCollector::new)
            .insert(round, ProposalWithHash::new(proposal, hash))
            .map_err(|_| ConsensusError::MultiProposal(epoch_id, round))?;
        Ok(())
    }

    pub fn flush(&mut self, till: u64) {
        self.0.split_off(&till);
    }

    pub fn get(&self, epoch_id: u64, round: u64) -> ConsensusResult<(SignedProposal<T>, Hash)> {
        if let Some(round_collector) = self.0.get(&epoch_id) {
            let res = round_collector
                .get(round)
                .map_err(|_| {
                    ConsensusError::StorageErr(format!(
                        "No proposal epoch ID {}, round {}",
                        epoch_id, round
                    ))
                })?
                .to_owned();
            return Ok((res.proposal, res.hash));
        }

        Err(ConsensusError::StorageErr(format!(
            "No proposal epoch ID {}, round {}",
            epoch_id, round
        )))
    }
}

/// A struct to collect signed proposals in each round. It stores each round and the corresponding
/// signed proposals in a `HashMap`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProposalRoundCollector<T: Codec>(HashMap<u64, ProposalWithHash<T>>);

impl<T> ProposalRoundCollector<T>
where
    T: Codec,
{
    fn new() -> Self {
        ProposalRoundCollector(HashMap::new())
    }

    fn insert(&mut self, round: u64, proposal: ProposalWithHash<T>) -> ConsensusResult<()> {
        if self.0.get(&round).is_some() {
            return Err(ConsensusError::Other("_".to_string()));
        }
        self.insert(round, proposal);
        Ok(())
    }

    fn get(&self, round: u64) -> ConsensusResult<&ProposalWithHash<T>> {
        self.0
            .get(&round)
            .ok_or_else(|| ConsensusError::StorageErr("_".to_string()))
    }
}

/// A signed proposal with its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProposalWithHash<T: Codec> {
    proposal: SignedProposal<T>,
    hash:     Hash,
}

impl<T> ProposalWithHash<T>
where
    T: Codec,
{
    pub fn new(proposal: SignedProposal<T>, hash: Hash) -> Self {
        ProposalWithHash { proposal, hash }
    }
}

///
pub struct VoteCollector(BTreeMap<u64, VoteRoundCollector>);

impl VoteCollector {
    pub fn new() -> Self {
        VoteCollector(BTreeMap::new())
    }

    pub fn insert_vote(
        &mut self,
        vote: SignedVote,
        epoch: u64,
        round: u64,
        addr: Address,
        vote_type: VoteType,
    ) {
        self.0
            .entry(epoch)
            .or_insert_with(VoteRoundCollector::new)
            .insert_vote(vote);
    }
}

///
struct VoteRoundCollector(HashMap<u64, RoundCollector>);

impl VoteRoundCollector {
    fn new() -> Self {
        VoteRoundCollector(HashMap::new())
    }

    fn insert(&mut self, vote: SignedVote, round: u64, vote_type: VoteType) {
        self.0
            .entry(round)
            .or_insert_with(RoundCollector::new)
            .insert(vote, vote_type);
    }
}

///
struct RoundCollector {
    qc:                  QuorumCertificate,
    prevote_collector:   HashMap<Hash, Vec<Address>>,
    precommit_collector: HashMap<Hash, Vec<Address>>,
}

impl VoteRoundCollector {
    fn new() -> Self {
        VoteRoundCollector {
            qc:                  QuorumCertificate::new(),
            prevote_collector:   HashMap::new(),
            precommit_collector: HashMap::new(),
        }
    }

    fn insert_vote(&mut self, vote: SignedVote, vote_type: VoteType) {
        match vote_type {
            VoteType::Prevote => self.
        }
    }
}

/// A struct includes precoteQC and precommitQC in a round.
struct QuorumCertificate {
    prevote:   Option<AggregatedVote>,
    precommit: Option<AggregatedVote>,
}

impl QuorumCertificate {
    fn new() -> Self {
        QuorumCertificate { None, None }
    }

    fn set_prevote_qc(&mut self, qc: AggregatedVote) {
        self.prevote = Some(qc);
    }

    fn set_precommit_qc(&mut self, qc: AggregatedVote) {
        self.precommit = Some(qc);
    }
}
