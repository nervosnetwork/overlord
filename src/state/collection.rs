use std::collections::{BTreeMap, HashMap, HashSet};

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
    /// Create a new proposal collector.
    pub fn new() -> Self {
        ProposalCollector(BTreeMap::new())
    }

    /// Insert a signed proposal into the proposal collector. Return `Err()` while the proposal of
    /// the given epoch ID and round exists.
    pub fn insert(
        &mut self,
        epoch_id: u64,
        round: u64,
        proposal: SignedProposal<T>,
    ) -> ConsensusResult<()> {
        self.0
            .entry(epoch_id)
            .or_insert_with(ProposalRoundCollector::new)
            .insert(round, proposal)
            .map_err(|_| ConsensusError::MultiProposal(epoch_id, round))
    }

    /// Get the signed proposal of the given epoch ID and round. Return `Err` when there is no
    /// signed proposal. Return `Err` when can not get it.
    pub fn get(&self, epoch_id: u64, round: u64) -> ConsensusResult<SignedProposal<T>> {
        if let Some(round_collector) = self.0.get(&epoch_id) {
            return Ok(round_collector
                .get(round)
                .map_err(|_| {
                    ConsensusError::StorageErr(format!(
                        "No proposal epoch ID {}, round {}",
                        epoch_id, round
                    ))
                })?
                .to_owned());
        }

        Err(ConsensusError::StorageErr(format!(
            "No proposal epoch ID {}, round {}",
            epoch_id, round
        )))
    }

    /// Get all proposals of the given epoch ID.
    pub fn get_epoch_proposals(&mut self, epoch_id: u64) -> Option<Vec<SignedProposal<T>>> {
        self.0.remove(&epoch_id).map_or_else(
            || None,
            |map| Some(map.0.values().cloned().collect::<Vec<_>>()),
        )
    }

    /// Remove items that epoch ID is less than `till`.
    pub fn flush(&mut self, till: u64) {
        self.0 = self.0.split_off(&till);
    }
}

/// A struct to collect signed proposals in each round. It stores each round and the corresponding
/// signed proposals in a `HashMap`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProposalRoundCollector<T: Codec>(HashMap<u64, SignedProposal<T>>);

impl<T> ProposalRoundCollector<T>
where
    T: Codec,
{
    fn new() -> Self {
        ProposalRoundCollector(HashMap::new())
    }

    fn insert(&mut self, round: u64, proposal: SignedProposal<T>) -> ConsensusResult<()> {
        if self.0.get(&round).is_some() {
            return Err(ConsensusError::Other("_".to_string()));
        }
        self.0.insert(round, proposal);
        Ok(())
    }

    fn get(&self, round: u64) -> ConsensusResult<&SignedProposal<T>> {
        self.0
            .get(&round)
            .ok_or_else(|| ConsensusError::StorageErr("_".to_string()))
    }
}

/// A struct to collect votes in each epoch. It stores each epoch and the corresponding votes in a
/// `BTreeMap`. The votes includes aggregated vote and signed vote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoteCollector(BTreeMap<u64, VoteRoundCollector>);

impl VoteCollector {
    /// Create a new vote collector.
    pub fn new() -> Self {
        VoteCollector(BTreeMap::new())
    }

    /// Insert a vote to the collector.
    pub fn insert_vote(&mut self, hash: Hash, vote: SignedVote, addr: Address) {
        self.0
            .entry(vote.get_epoch())
            .or_insert_with(VoteRoundCollector::new)
            .insert_vote(hash, vote, addr);
    }

    /// Set a given quorum certificate to the collector.
    pub fn set_qc(&mut self, qc: AggregatedVote) {
        self.0
            .entry(qc.get_epoch())
            .or_insert_with(VoteRoundCollector::new)
            .set_qc(qc);
    }

    /// Get an index of a `HashMap` that the key is vote hash and the value is address list, with
    /// the given epoch ID, round and type.
    pub fn get_vote_map(
        &mut self,
        epoch: u64,
        round: u64,
        vote_type: VoteType,
    ) -> ConsensusResult<&HashMap<Hash, HashSet<Address>>> {
        self.0
            .get_mut(&epoch)
            .and_then(|vrc| vrc.get_vote_map(round, vote_type.clone()))
            .ok_or_else(|| {
                ConsensusError::StorageErr(format!(
                    "Can not get {:?} vote map epoch ID {}, round {}",
                    vote_type, epoch, round
                ))
            })
    }

    /// Get a vote list with the given epoch, round, type and hash.
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

    /// Get a quorum certificate with the given epoch, round and type.
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

    /// Get all votes and quorum certificates of the given epoch ID.
    pub fn get_epoch_votes(
        &mut self,
        epoch_id: u64,
    ) -> Option<(Vec<SignedVote>, Vec<AggregatedVote>)> {
        self.0.remove(&epoch_id).map_or_else(
            || None,
            |mut vrc| {
                let mut votes = Vec::new();
                let mut qcs = Vec::new();

                for (_, rc) in vrc.0.iter_mut() {
                    votes.append(&mut rc.prevote.get_all_votes());
                    votes.append(&mut rc.precommit.get_all_votes());
                    qcs.append(&mut rc.qc.get_all_qcs());
                }
                Some((votes, qcs))
            },
        )
    }

    /// Remove items that epoch ID is less than `till`.
    pub fn flush(&mut self, till: u64) {
        self.0 = self.0.split_off(&till);
    }
}

/// A struct to collect votes in each round.  It stores each round votes and the corresponding votes
/// in a `HashMap`.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    fn get_vote_map(
        &mut self,
        round: u64,
        vote_type: VoteType,
    ) -> Option<&HashMap<Hash, HashSet<Address>>> {
        self.0.get_mut(&round).and_then(|rc| {
            let res = rc.get_vote_map(vote_type);
            if res.is_empty() {
                return None;
            }
            Some(res)
        })
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

/// A round collector contains a qc and prevote votes and precommit votes.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    fn get_vote_map(&self, vote_type: VoteType) -> &HashMap<Hash, HashSet<Address>> {
        match vote_type {
            VoteType::Prevote => self.prevote.get_vote_map(),
            VoteType::Precommit => self.precommit.get_vote_map(),
        }
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

/// A struct includes prevoteQC and precommitQC in a round.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    fn get_all_qcs(&mut self) -> Vec<AggregatedVote> {
        let mut res = Vec::new();

        if let Some(tmp) = self.prevote.clone() {
            res.push(tmp);
        }

        if let Some(tmp) = self.precommit.clone() {
            res.push(tmp);
        }
        res
    }
}

///
#[derive(Clone, Debug, PartialEq, Eq)]
struct Votes {
    by_hash:    HashMap<Hash, HashSet<Address>>,
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
            .or_insert_with(HashSet::new)
            .insert(addr.clone());
        self.by_address.entry(addr).or_insert(vote);
    }

    fn get_vote_map(&self) -> &HashMap<Hash, HashSet<Address>> {
        &self.by_hash
    }

    fn get_votes(&mut self, hash: &Hash) -> Option<Vec<SignedVote>> {
        self.by_hash.get(hash).and_then(|addresses| {
            addresses
                .iter()
                .map(|addr| self.by_address.get(addr).cloned())
                .collect::<Option<Vec<_>>>()
        })
    }

    fn get_all_votes(&mut self) -> Vec<SignedVote> {
        self.by_address.values().cloned().collect::<Vec<_>>()
    }
}

#[cfg(test)]
mod test {
    extern crate test;

    use std::collections::{HashMap, HashSet};
    use std::error::Error;

    use bincode::{deserialize, serialize};
    use bytes::Bytes;
    use rand::random;
    use serde::{Deserialize, Serialize};
    use test::Bencher;

    use crate::state::collection::{ProposalCollector, VoteCollector};
    use crate::types::{
        Address, AggregatedSignature, AggregatedVote, Hash, Proposal, Signature, SignedProposal,
        SignedVote, Vote, VoteType,
    };
    use crate::Codec;

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
    struct Pill {
        epoch_id: u64,
        epoch:    Vec<u64>,
    }

    impl Codec for Pill {
        fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
            let encode: Vec<u8> = serialize(&self).expect("Serialize Pill error");
            Ok(Bytes::from(encode))
        }

        fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>> {
            let decode: Pill = deserialize(&data.as_ref()).expect("Deserialize Pill error.");
            Ok(decode)
        }
    }

    impl Pill {
        fn new() -> Self {
            let epoch_id = random::<u64>();
            let epoch = (0..128).map(|_| random::<u64>()).collect::<Vec<_>>();
            Pill { epoch_id, epoch }
        }
    }

    fn gen_hash() -> Hash {
        Hash::from((0..16).map(|_| random::<u8>()).collect::<Vec<_>>())
    }

    fn gen_address() -> Address {
        Address::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>())
    }

    fn gen_signature() -> Signature {
        Signature::from((0..64).map(|_| random::<u8>()).collect::<Vec<_>>())
    }

    fn gen_aggr_signature() -> AggregatedSignature {
        AggregatedSignature {
            signature:      gen_signature(),
            address_bitmap: Bytes::from((0..8).map(|_| random::<u8>()).collect::<Vec<_>>()),
        }
    }

    fn gen_signed_proposal(epoch_id: u64, round: u64) -> SignedProposal<Pill> {
        let signature = gen_signature();
        let proposal = Proposal {
            epoch_id,
            round,
            content: Pill::new(),
            epoch_hash: gen_hash(),
            lock: None,
            proposer: gen_address(),
        };

        SignedProposal {
            signature,
            proposal,
        }
    }

    fn gen_signed_vote(
        epoch_id: u64,
        round: u64,
        vote_type: VoteType,
        hash: Hash,
        addr: Address,
    ) -> SignedVote {
        let vote = Vote {
            epoch_id,
            round,
            vote_type,
            epoch_hash: hash,
            voter: addr,
        };

        SignedVote {
            signature: gen_signature(),
            vote,
        }
    }

    fn gen_aggregated_vote(epoch_id: u64, round: u64, vote_type: VoteType) -> AggregatedVote {
        let signature = AggregatedSignature {
            signature:      gen_signature(),
            address_bitmap: gen_address(),
        };

        AggregatedVote {
            signature,
            epoch_id,
            round,
            vote_type,
            epoch_hash: gen_hash(),
            leader: gen_address(),
        }
    }

    #[test]
    fn test_proposal_collector() {
        let mut proposals = ProposalCollector::<Pill>::new();
        let proposal_01 = gen_signed_proposal(1, 0);
        let proposal_02 = gen_signed_proposal(1, 0);

        assert!(proposals.insert(1, 0, proposal_01.clone()).is_ok());
        assert!(proposals.insert(1, 0, proposal_02).is_err());
        assert_eq!(proposals.get(1, 0).unwrap(), proposal_01);

        let proposal_03 = gen_signed_proposal(2, 0);
        let proposal_04 = gen_signed_proposal(3, 0);

        assert!(proposals.insert(2, 0, proposal_03.clone()).is_ok());
        assert!(proposals.insert(3, 0, proposal_04.clone()).is_ok());

        proposals.flush(2);
        assert!(proposals.get(1, 0).is_err());
        assert_eq!(proposals.get(2, 0).unwrap(), proposal_03.clone());
        assert_eq!(proposals.get(3, 0).unwrap(), proposal_04);

        assert!(proposals.get_epoch_proposals(1).is_none());
        assert_eq!(proposals.get_epoch_proposals(2).unwrap(), vec![proposal_03]);
        assert!(proposals.get(2, 0).is_err());
    }

    #[test]
    fn test_vote_collector() {
        let mut votes = VoteCollector::new();

        let mut map = HashMap::new();
        let mut vec = Vec::new();
        let mut set = HashSet::new();

        let hash_01 = gen_hash();
        let hash_02 = gen_hash();
        let addr_01 = gen_address();
        let addr_02 = gen_address();
        let signed_vote_01 =
            gen_signed_vote(1, 0, VoteType::Prevote, hash_01.clone(), addr_01.clone());
        let signed_vote_02 =
            gen_signed_vote(1, 0, VoteType::Prevote, hash_01.clone(), addr_02.clone());

        votes.insert_vote(hash_01.clone(), signed_vote_01.clone(), addr_01.clone());

        set.insert(addr_01.clone());
        map.insert(hash_01.clone(), set);
        vec.push(signed_vote_01);

        assert_eq!(votes.get_vote_map(1, 0, VoteType::Prevote), Ok(&map));
        assert_eq!(
            votes.get_votes(1, 0, VoteType::Prevote, &hash_01),
            Ok(vec.clone())
        );
        assert!(votes.get_vote_map(1, 0, VoteType::Precommit).is_err());
        assert!(votes
            .get_votes(1, 0, VoteType::Precommit, &hash_01)
            .is_err());
        assert!(votes.get_vote_map(1, 1, VoteType::Prevote).is_err());
        assert!(votes.get_votes(1, 1, VoteType::Prevote, &hash_01).is_err());
        assert!(votes.get_votes(1, 0, VoteType::Prevote, &hash_02).is_err());

        votes.insert_vote(hash_01.clone(), signed_vote_02.clone(), addr_02.clone());
        map.get_mut(&hash_01).unwrap().insert(addr_02.clone());
        vec.push(signed_vote_02);

        assert_eq!(votes.get_vote_map(1, 0, VoteType::Prevote), Ok(&map));
        let res = votes
            .get_votes(1, 0, VoteType::Prevote, &hash_01)
            .unwrap()
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        assert_eq!(res, vec.iter().cloned().collect::<HashSet<_>>());
    }

    #[bench]
    fn bench_insert_proposal(b: &mut Bencher) {
        let mut proposals = ProposalCollector::<Pill>::new();
        let proposal = gen_signed_proposal(1, 0);
        b.iter(|| proposals.insert(1, 0, proposal.clone()));
    }

    #[bench]
    fn bench_insert_vote(b: &mut Bencher) {
        let mut votes = VoteCollector::new();
        let hash = gen_hash();
        let addr = gen_address();
        let sv = gen_signed_vote(
            random::<u64>(),
            random::<u64>(),
            VoteType::Prevote,
            hash.clone(),
            addr.clone(),
        );
        b.iter(|| votes.insert_vote(hash.clone(), sv.clone(), addr.clone()))
    }

    #[bench]
    fn bench_insert_qc(b: &mut Bencher) {
        let mut votes = VoteCollector::new();
        let av = gen_aggregated_vote(random::<u64>(), random::<u64>(), VoteType::Prevote);
        b.iter(|| votes.set_qc(av.clone()));
    }

    #[bench]
    fn bench_get_votes(b: &mut Bencher) {
        let mut votes = VoteCollector::new();
        let hash = gen_hash();
        for _ in 0..10 {
            let addr = gen_address();
            let sv = gen_signed_vote(1, 0, VoteType::Prevote, hash.clone(), addr.clone());
            votes.insert_vote(hash.clone(), sv, addr);
        }
        b.iter(|| votes.get_votes(1, 0, VoteType::Prevote, &hash));
    }
}
