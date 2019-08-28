use std::collections::{BTreeMap, HashMap};

use crate::types::{Address, AggregatedVote, Hash, SignedProposal, SignedVote, VoteType};
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
        self.0.insert(round, proposal);
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

    pub fn insert_vote(&mut self, hash: Hash, vote: SignedVote, addr: Address) {
        self.0
            .entry(vote.get_epoch())
            .or_insert_with(VoteRoundCollector::new)
            .insert_vote(hash, vote, addr);
    }

    pub fn set_qc(&mut self, qc: AggregatedVote) {
        self.0
            .entry(qc.get_epoch())
            .or_insert_with(VoteRoundCollector::new)
            .set_qc(qc);
    }

    pub fn remove(&mut self, till: u64) {
        self.0.split_off(&till);
    }

    pub fn get_votes(
        &mut self,
        epoch: u64,
        round: u64,
        vote_type: VoteType,
        hash: &Hash,
    ) -> ConsensusResult<Vec<SignedVote>> {
        self.0
            .get_mut(&epoch)
            .and_then(|vrc| vrc.get_votes(round, vote_type.clone(), hash))
            .ok_or_else(|| {
                ConsensusError::StorageErr(format!(
                    "Can not get {:?} votes epoch ID {}, round {}",
                    vote_type, epoch, round
                ))
            })
    }

    pub fn get_qc(
        &mut self,
        epoch: u64,
        round: u64,
        qc_type: VoteType,
    ) -> ConsensusResult<AggregatedVote> {
        self.0
            .get_mut(&epoch)
            .and_then(|vrc| vrc.get_qc(round, qc_type.clone()))
            .ok_or_else(|| {
                ConsensusError::StorageErr(format!(
                    "Can not get {:?} qc epoch ID {}, round {}",
                    qc_type, epoch, round
                ))
            })
    }
}

///
struct VoteRoundCollector(HashMap<u64, RoundCollector>);

impl VoteRoundCollector {
    fn new() -> Self {
        VoteRoundCollector(HashMap::new())
    }

    fn insert_vote(&mut self, hash: Hash, vote: SignedVote, addr: Address) {
        self.0
            .entry(vote.get_round())
            .or_insert_with(RoundCollector::new)
            .insert_vote(hash, vote, addr);
    }

    fn set_qc(&mut self, qc: AggregatedVote) {
        self.0
            .entry(qc.get_round())
            .or_insert_with(RoundCollector::new)
            .set_qc(qc);
    }

    fn get_votes(
        &mut self,
        round: u64,
        vote_type: VoteType,
        hash: &Hash,
    ) -> Option<Vec<SignedVote>> {
        self.0
            .get_mut(&round)
            .and_then(|rc| rc.get_votes(vote_type, hash))
    }

    fn get_qc(&mut self, round: u64, qc_type: VoteType) -> Option<AggregatedVote> {
        self.0.get_mut(&round).and_then(|rc| rc.get_qc(qc_type))
    }
}

///
struct RoundCollector {
    qc:        QuorumCertificate,
    prevote:   Votes,
    precommit: Votes,
}

impl RoundCollector {
    fn new() -> Self {
        RoundCollector {
            qc:        QuorumCertificate::new(),
            prevote:   Votes::new(),
            precommit: Votes::new(),
        }
    }

    fn insert_vote(&mut self, hash: Hash, vote: SignedVote, addr: Address) {
        if vote.is_prevote() {
            self.prevote.insert(hash, addr, vote);
        } else {
            self.precommit.insert(hash, addr, vote);
        }
    }

    fn set_qc(&mut self, qc: AggregatedVote) {
        self.qc.set_quorum_certificate(qc);
    }

    fn get_votes(&mut self, vote_type: VoteType, hash: &Hash) -> Option<Vec<SignedVote>> {
        match vote_type {
            VoteType::Prevote => self.prevote.get_votes(hash),
            VoteType::Precommit => self.precommit.get_votes(hash),
        }
    }

    fn get_qc(&mut self, qc_type: VoteType) -> Option<AggregatedVote> {
        self.qc.get_quorum_certificate(qc_type)
    }
}

/// A struct includes precoteQC and precommitQC in a round.
struct QuorumCertificate {
    prevote:   Option<AggregatedVote>,
    precommit: Option<AggregatedVote>,
}

impl QuorumCertificate {
    fn new() -> Self {
        QuorumCertificate {
            prevote:   None,
            precommit: None,
        }
    }

    fn set_quorum_certificate(&mut self, qc: AggregatedVote) {
        if qc.is_prevote_qc() {
            self.prevote = Some(qc);
        } else {
            self.precommit = Some(qc);
        }
    }

    fn get_quorum_certificate(&mut self, qc_type: VoteType) -> Option<AggregatedVote> {
        match qc_type {
            VoteType::Prevote => self.prevote.clone(),
            VoteType::Precommit => self.precommit.clone(),
        }
    }
}

struct Votes {
    by_hash:    HashMap<Hash, Vec<Address>>,
    by_address: HashMap<Address, SignedVote>,
}

impl Votes {
    fn new() -> Self {
        Votes {
            by_hash:    HashMap::new(),
            by_address: HashMap::new(),
        }
    }

    fn insert(&mut self, hash: Hash, addr: Address, vote: SignedVote) {
        self.by_hash
            .entry(hash)
            .or_insert_with(Vec::new)
            .push(addr.clone());
        self.by_address.entry(addr).or_insert(vote);
    }

    fn get(&self) -> &HashMap<Hash, Vec<Address>> {
        &self.by_hash
    }

    fn get_votes(&mut self, hash: &Hash) -> Option<Vec<SignedVote>> {
        let mut res = Vec::new();
        if let Some(address) = self.by_hash.get(hash) {
            for addr in address.iter() {
                res.push(self.by_address.get(addr).unwrap().to_owned());
            }
            return Some(res);
        }
        None
    }
}
