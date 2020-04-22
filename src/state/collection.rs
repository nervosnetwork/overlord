use std::collections::{BTreeMap, HashMap, HashSet};

use crate::types::{
    Address, AggregatedChoke, AggregatedVote, Hash, SignedChoke, SignedProposal, SignedVote,
    VoteType,
};
use crate::{error::ConsensusError, Codec, ConsensusResult};

/// A struct to collect signed proposals in each height. It stores each height and the corresponding
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
    /// the given height and round exists.
    pub fn insert(
        &mut self,
        height: u64,
        round: u64,
        proposal: SignedProposal<T>,
    ) -> ConsensusResult<()> {
        self.0
            .entry(height)
            .or_insert_with(ProposalRoundCollector::new)
            .insert(round, proposal)
            .map_err(|_| ConsensusError::MultiProposal(height, round))
    }

    /// Get the signed proposal of the given height and round. Return `Err` when there is no
    /// signed proposal. Return `Err` when can not get it.
    pub fn get(&self, height: u64, round: u64) -> ConsensusResult<SignedProposal<T>> {
        if let Some(round_collector) = self.0.get(&height) {
            return Ok(round_collector
                .get(round)
                .map_err(|_| {
                    ConsensusError::StorageErr(format!(
                        "No proposal height {}, round {}",
                        height, round
                    ))
                })?
                .to_owned());
        }

        Err(ConsensusError::StorageErr(format!(
            "No proposal height {}, round {}",
            height, round
        )))
    }

    /// Get all proposals of the given height.
    pub fn get_height_proposals(&mut self, height: u64) -> Option<Vec<SignedProposal<T>>> {
        self.0.remove(&height).map_or_else(
            || None,
            |map| Some(map.0.values().cloned().collect::<Vec<_>>()),
        )
    }

    /// Remove items that height is less than `till`.
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
        if let Some(sp) = self.0.get(&round) {
            if sp == &proposal {
                return Ok(());
            }
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

/// A struct to collect votes in each height. It stores each height and the corresponding votes in a
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
            .entry(vote.get_height())
            .or_insert_with(VoteRoundCollector::new)
            .insert_vote(hash, vote, addr);
    }

    /// Set a given quorum certificate to the collector.
    pub fn set_qc(&mut self, qc: AggregatedVote) {
        self.0
            .entry(qc.get_height())
            .or_insert_with(VoteRoundCollector::new)
            .set_qc(qc);
    }

    /// Get an index of a `HashMap` that the key is vote hash and the value is address list, with
    /// the given height, round and type.
    pub fn get_vote_map(
        &mut self,
        height: u64,
        round: u64,
        vote_type: VoteType,
    ) -> ConsensusResult<&HashMap<Hash, HashSet<Address>>> {
        self.0
            .get_mut(&height)
            .and_then(|vrc| vrc.get_vote_map(round, vote_type.clone()))
            .ok_or_else(|| {
                ConsensusError::StorageErr(format!(
                    "Can not get {:?} vote map height {}, round {}",
                    vote_type, height, round
                ))
            })
    }

    /// Get a vote list with the given height, round, type and hash.
    pub fn get_votes(
        &mut self,
        height: u64,
        round: u64,
        vote_type: VoteType,
        hash: &Hash,
    ) -> ConsensusResult<Vec<SignedVote>> {
        self.0
            .get_mut(&height)
            .and_then(|vrc| vrc.get_votes(round, vote_type.clone(), hash))
            .ok_or_else(|| {
                ConsensusError::StorageErr(format!(
                    "Can not get {:?} votes height {}, round {}",
                    vote_type, height, round
                ))
            })
    }

    /// Get a quorum certificate with the given height, round and type.
    pub fn get_qc_by_id(
        &mut self,
        height: u64,
        round: u64,
        qc_type: VoteType,
    ) -> ConsensusResult<AggregatedVote> {
        self.0
            .get_mut(&height)
            .and_then(|vrc| vrc.get_qc_by_id(round, qc_type.clone()))
            .ok_or_else(|| {
                ConsensusError::StorageErr(format!(
                    "Can not get {:?} qc height {}, round {}",
                    qc_type, height, round
                ))
            })
    }

    pub fn get_qc_by_hash(
        &mut self,
        height: u64,
        hash: Hash,
        qc_type: VoteType,
    ) -> Option<AggregatedVote> {
        self.0
            .get_mut(&height)
            .and_then(|vrc| vrc.get_qc_by_hash(hash, qc_type))
    }

    /// Get all votes and quorum certificates of the given height.
    pub fn get_height_votes(
        &mut self,
        height: u64,
    ) -> Option<(Vec<SignedVote>, Vec<AggregatedVote>)> {
        self.0.remove(&height).map_or_else(
            || None,
            |mut vrc| {
                let mut votes = Vec::new();
                let mut qcs = Vec::new();

                for (_, rc) in vrc.general.iter_mut() {
                    votes.append(&mut rc.prevote.get_all_votes());
                    votes.append(&mut rc.precommit.get_all_votes());
                    qcs.append(&mut rc.qc.get_all_qcs());
                }
                Some((votes, qcs))
            },
        )
    }

    pub fn vote_count(&self, height: u64, round: u64, vote_type: VoteType) -> usize {
        if let Some(vrc) = self.0.get(&height) {
            return vrc.vote_count(round, vote_type);
        }
        0
    }

    /// Remove items that height is less than `till`.
    pub fn flush(&mut self, till: u64) {
        self.0 = self.0.split_off(&till);
    }
}

/// A struct to collect votes in each round.  It stores each round votes and the corresponding votes
/// in a `HashMap`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct VoteRoundCollector {
    general:    HashMap<u64, RoundCollector>,
    qc_by_hash: HashMap<Hash, QuorumCertificate>,
}

impl VoteRoundCollector {
    fn new() -> Self {
        VoteRoundCollector {
            general:    HashMap::new(),
            qc_by_hash: HashMap::new(),
        }
    }

    fn insert_vote(&mut self, hash: Hash, vote: SignedVote, addr: Address) {
        self.general
            .entry(vote.get_round())
            .or_insert_with(RoundCollector::new)
            .insert_vote(hash, vote, addr);
    }

    fn set_qc(&mut self, qc: AggregatedVote) {
        self.qc_by_hash
            .entry(qc.block_hash.clone())
            .or_insert_with(QuorumCertificate::new)
            .set_quorum_certificate(qc.clone());

        self.general
            .entry(qc.get_round())
            .or_insert_with(RoundCollector::new)
            .set_qc(qc);
    }

    fn get_vote_map(
        &mut self,
        round: u64,
        vote_type: VoteType,
    ) -> Option<&HashMap<Hash, HashSet<Address>>> {
        self.general.get_mut(&round).and_then(|rc| {
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
        self.general
            .get_mut(&round)
            .and_then(|rc| rc.get_votes(vote_type, hash))
    }

    fn get_qc_by_id(&mut self, round: u64, qc_type: VoteType) -> Option<AggregatedVote> {
        self.general
            .get_mut(&round)
            .and_then(|rc| rc.get_qc(qc_type))
    }

    fn get_qc_by_hash(&self, hash: Hash, qc_type: VoteType) -> Option<AggregatedVote> {
        if let Some(qcs) = self.qc_by_hash.get(&hash) {
            return qcs.get_quorum_certificate(qc_type);
        }
        None
    }

    fn vote_count(&self, round: u64, vote_type: VoteType) -> usize {
        if let Some(rc) = self.general.get(&round) {
            return rc.vote_count(vote_type);
        }
        0
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

    fn vote_count(&self, vote_type: VoteType) -> usize {
        if vote_type == VoteType::Prevote {
            return self.prevote.vote_count();
        }
        self.precommit.vote_count()
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

    fn get_quorum_certificate(&self, qc_type: VoteType) -> Option<AggregatedVote> {
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
        if self.by_address.contains_key(&addr) {
            // the addr somehow has already inserted a Vote we ignore the incoming SignedVote no
            // matter it duplicates or differs(byzantine), reject the current request!
            let exist = self.by_address.get(&addr).unwrap().clone();
            if !vote.vote.block_hash.eq(&exist.vote.block_hash) {
                // this is a byzantine behaviour
                log::error!("Overlord: VoteCollector detects byzantine behaviour: existing: {}, signed vote inserting: {}",
                exist,vote);
            }
            return;
        }

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

    fn vote_count(&self) -> usize {
        self.by_address.len()
    }
}

#[derive(Clone, Debug)]
pub struct ChokeCollector {
    chokes: BTreeMap<u64, HashSet<SignedChoke>>,
    qcs:    HashMap<u64, AggregatedChoke>,
}

impl ChokeCollector {
    pub fn new() -> Self {
        ChokeCollector {
            chokes: BTreeMap::new(),
            qcs:    HashMap::new(),
        }
    }

    pub fn insert(&mut self, round: u64, signed_choke: SignedChoke) {
        self.chokes
            .entry(round)
            .or_insert_with(HashSet::new)
            .insert(signed_choke);
    }

    pub fn set_qc(&mut self, round: u64, qc: AggregatedChoke) {
        self.qcs.insert(round, qc);
    }

    pub fn get_chokes(&self, round: u64) -> Option<Vec<SignedChoke>> {
        self.chokes
            .get(&round)
            .map(|set| set.iter().cloned().collect::<Vec<_>>())
    }

    pub fn get_qc(&self, round: u64) -> Option<AggregatedChoke> {
        self.qcs.get(&round).cloned()
    }

    pub fn max_round_above_threshold(&self, nodes_num: usize) -> Option<u64> {
        for (round, set) in self.chokes.iter().rev() {
            if set.len() * 3 > nodes_num * 2 {
                return Some(*round);
            }
        }
        None
    }

    pub fn print_round_choke_log(&self, round: u64) {
        if let Some(set) = self.chokes.get(&round) {
            let num = set.len();
            let voters = set
                .iter()
                .map(|sc| hex::encode(sc.address.clone()))
                .collect::<Vec<_>>();
            log::info!(
                "Overlord: {} chokes in round {}, voters {:?}",
                num,
                round,
                voters
            );
        } else {
            log::info!("Overlord: no choke in round {}", round);
        }
    }

    pub fn clear(&mut self) {
        self.chokes.clear();
        self.qcs.clear();
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
        height: u64,
        epoch:  Vec<u64>,
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
            let height = random::<u64>();
            let epoch = (0..128).map(|_| random::<u64>()).collect::<Vec<_>>();
            Pill { height, epoch }
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

    fn gen_signed_proposal(height: u64, round: u64) -> SignedProposal<Pill> {
        let signature = gen_signature();
        let proposal = Proposal {
            height,
            round,
            content: Pill::new(),
            block_hash: gen_hash(),
            lock: None,
            proposer: gen_address(),
        };

        SignedProposal {
            signature,
            proposal,
        }
    }

    fn gen_signed_vote(
        height: u64,
        round: u64,
        vote_type: VoteType,
        hash: Hash,
        addr: Address,
    ) -> SignedVote {
        let vote = Vote {
            height,
            round,
            vote_type,
            block_hash: hash,
        };

        SignedVote {
            signature: gen_signature(),
            voter: addr,
            vote,
        }
    }

    fn gen_aggregated_vote(height: u64, round: u64, vote_type: VoteType) -> AggregatedVote {
        let signature = gen_aggr_signature();

        AggregatedVote {
            signature,
            height,
            round,
            vote_type,
            block_hash: gen_hash(),
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

        assert!(proposals.get_height_proposals(1).is_none());
        assert_eq!(proposals.get_height_proposals(2).unwrap(), vec![
            proposal_03
        ]);
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

        set.insert(addr_01);
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
        map.get_mut(&hash_01).unwrap().insert(addr_02);
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
