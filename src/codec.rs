use std::convert::TryFrom;

use bytes::Bytes;
use rlp::{Decodable, DecoderError, Encodable, Prototype, Rlp, RlpStream};

use crate::smr::smr_types::Step;
use crate::types::{
    Address, AggregatedChoke, AggregatedSignature, AggregatedVote, Choke, Commit, Hash, HashChoke,
    Node, PoLC, Proof, Proposal, Signature, SignedChoke, SignedProposal, SignedVote, Status,
    UpdateFrom, Vote, VoteType,
};
use crate::wal::{WalInfo, WalLock};
use crate::{Codec, DurationConfig};

// impl Encodable and Decodable trait for SignedProposal
impl<T: Codec> Encodable for SignedProposal<T> {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2)
            .append(&self.signature.to_vec())
            .append(&self.proposal);
    }
}

impl<T: Codec> Decodable for SignedProposal<T> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let signature = Signature::from(tmp);
                let proposal: Proposal<T> = r.val_at(1)?;
                Ok(SignedProposal {
                    signature,
                    proposal,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Proposal
impl<T: Codec> Encodable for Proposal<T> {
    fn rlp_append(&self, s: &mut RlpStream) {
        let content = self.content.encode().unwrap().to_vec();
        s.begin_list(6)
            .append(&self.height)
            .append(&self.round)
            .append(&self.block_hash.to_vec())
            .append(&self.lock)
            .append(&self.proposer.to_vec())
            .append(&content);
    }
}

impl<T: Codec> Decodable for Proposal<T> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(6) => {
                let height: u64 = r.val_at(0)?;
                let round: u64 = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let block_hash = Hash::from(tmp);
                let lock = r.val_at(3)?;
                let tmp: Vec<u8> = r.val_at(4)?;
                let proposer = Address::from(tmp);
                let tmp: Vec<u8> = r.val_at(5)?;
                let content = Codec::decode(Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(Proposal {
                    height,
                    round,
                    content,
                    block_hash,
                    lock,
                    proposer,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for PoLC
impl Encodable for PoLC {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2)
            .append(&self.lock_round)
            .append(&self.lock_votes);
    }
}

impl Decodable for PoLC {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let lock_round: u64 = r.val_at(0)?;
                let lock_votes: AggregatedVote = r.val_at(1)?;
                Ok(PoLC {
                    lock_round,
                    lock_votes,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for AggregatedSignature
impl Encodable for AggregatedSignature {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2)
            .append(&self.signature.to_vec())
            .append(&self.address_bitmap.to_vec());
    }
}

impl Decodable for AggregatedSignature {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let signature = Signature::from(tmp);
                let tmp: Vec<u8> = r.val_at(1)?;
                let address_bitmap = Bytes::from(tmp);
                Ok(AggregatedSignature {
                    signature,
                    address_bitmap,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for AggregatedVote
impl Encodable for AggregatedVote {
    fn rlp_append(&self, s: &mut RlpStream) {
        let vote_type: u8 = self.vote_type.clone().into();
        s.begin_list(6)
            .append(&self.signature)
            .append(&vote_type)
            .append(&self.height)
            .append(&self.round)
            .append(&self.block_hash.to_vec())
            .append(&self.leader.to_vec());
    }
}

impl Decodable for AggregatedVote {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(6) => {
                let signature: AggregatedSignature = r.val_at(0)?;
                let tmp: u8 = r.val_at(1)?;
                let vote_type = VoteType::try_from(tmp)
                    .map_err(|_| DecoderError::Custom("Invalid vote type"))?;
                let height: u64 = r.val_at(2)?;
                let round: u64 = r.val_at(3)?;
                let tmp: Vec<u8> = r.val_at(4)?;
                let block_hash = Hash::from(tmp);
                let tmp: Vec<u8> = r.val_at(5)?;
                let leader = Address::from(tmp);
                Ok(AggregatedVote {
                    signature,
                    vote_type,
                    height,
                    round,
                    block_hash,
                    leader,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for SignedVote
impl Encodable for SignedVote {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3)
            .append(&self.signature.to_vec())
            .append(&self.vote)
            .append(&self.voter.to_vec());
    }
}

impl Decodable for SignedVote {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let signature = Signature::from(tmp);
                let vote = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let voter = Address::from(tmp);
                Ok(SignedVote {
                    signature,
                    vote,
                    voter,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Vote
impl Encodable for Vote {
    fn rlp_append(&self, s: &mut RlpStream) {
        let vote_type: u8 = self.vote_type.clone().into();
        s.begin_list(4)
            .append(&self.height)
            .append(&self.round)
            .append(&vote_type)
            .append(&self.block_hash.to_vec());
    }
}

impl Decodable for Vote {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let height: u64 = r.val_at(0)?;
                let round: u64 = r.val_at(1)?;
                let tmp: u8 = r.val_at(2)?;
                let vote_type = VoteType::try_from(tmp)
                    .map_err(|_| DecoderError::Custom("Invalid vote type"))?;
                let tmp: Vec<u8> = r.val_at(3)?;
                let block_hash = Hash::from(tmp);
                Ok(Vote {
                    height,
                    round,
                    vote_type,
                    block_hash,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Commit
impl<T: Codec> Encodable for Commit<T> {
    fn rlp_append(&self, s: &mut RlpStream) {
        let content = self.content.encode().unwrap().to_vec();
        s.begin_list(3)
            .append(&self.height)
            .append(&self.proof)
            .append(&content);
    }
}

impl<T: Codec> Decodable for Commit<T> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let height: u64 = r.val_at(0)?;
                let proof: Proof = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let content = Codec::decode(Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(Commit {
                    height,
                    proof,
                    content,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Proof
impl Encodable for Proof {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(4)
            .append(&self.height)
            .append(&self.round)
            .append(&self.block_hash.to_vec())
            .append(&self.signature);
    }
}

impl Decodable for Proof {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let height: u64 = r.val_at(0)?;
                let round: u64 = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let block_hash = Hash::from(tmp);
                let signature: AggregatedSignature = r.val_at(3)?;
                Ok(Proof {
                    height,
                    round,
                    block_hash,
                    signature,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl Encodable for DurationConfig {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(4)
            .append(&self.propose_ratio)
            .append(&self.prevote_ratio)
            .append(&self.precommit_ratio)
            .append(&self.brake_ratio);
    }
}

impl Decodable for DurationConfig {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let propose_ratio: u64 = r.val_at(0)?;
                let prevote_ratio: u64 = r.val_at(1)?;
                let precommit_ratio: u64 = r.val_at(2)?;
                let brake_ratio: u64 = r.val_at(3)?;
                Ok(DurationConfig {
                    propose_ratio,
                    prevote_ratio,
                    precommit_ratio,
                    brake_ratio,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Status
impl Encodable for Status {
    fn rlp_append(&self, s: &mut RlpStream) {
        let interval = if self.interval.is_none() {
            0u64
        } else {
            self.interval.clone().unwrap()
        };

        let config = if self.timer_config.is_none() {
            DurationConfig::default()
        } else {
            self.timer_config.clone().unwrap()
        };
        s.begin_list(4)
            .append(&self.height)
            .append(&interval)
            .append(&config)
            .append_list(&self.authority_list);
    }
}

impl Decodable for Status {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let height: u64 = r.val_at(0)?;
                let tmp: u64 = r.val_at(1)?;
                let interval = if tmp == 0 { None } else { Some(tmp) };
                let tmp: DurationConfig = r.val_at(2)?;
                let timer_config = if tmp == DurationConfig::default() {
                    None
                } else {
                    Some(tmp)
                };
                let authority_list: Vec<Node> = r.list_at(3)?;

                Ok(Status {
                    height,
                    interval,
                    timer_config,
                    authority_list,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Node
impl Encodable for Node {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3)
            .append(&self.address.to_vec())
            .append(&self.propose_weight)
            .append(&self.vote_weight);
    }
}

impl Decodable for Node {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let address = Address::from(tmp);
                let propose_weight: u32 = r.val_at(1)?;
                let vote_weight: u32 = r.val_at(2)?;
                Ok(Node {
                    address,
                    propose_weight,
                    vote_weight,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl Encodable for UpdateFrom {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2);
        match self {
            UpdateFrom::PrevoteQC(qc) => {
                s.append(&0u8).append(qc);
            }
            UpdateFrom::PrecommitQC(qc) => {
                s.append(&1u8).append(qc);
            }
            UpdateFrom::ChokeQC(qc) => {
                s.append(&2u8).append(qc);
            }
        }
    }
}

impl Decodable for UpdateFrom {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let tmp: u8 = r.val_at(0)?;
                let res = match tmp {
                    0u8 => {
                        let qc: AggregatedVote = r.val_at(1)?;
                        UpdateFrom::PrevoteQC(qc)
                    }
                    1u8 => {
                        let qc: AggregatedVote = r.val_at(1)?;
                        UpdateFrom::PrecommitQC(qc)
                    }
                    2u8 => {
                        let qc: AggregatedChoke = r.val_at(1)?;
                        UpdateFrom::ChokeQC(qc)
                    }
                    _ => unreachable!(),
                };
                Ok(res)
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl<T: Codec> Encodable for WalLock<T> {
    fn rlp_append(&self, s: &mut RlpStream) {
        let content = self.content.encode().unwrap().to_vec();
        s.begin_list(3)
            .append(&self.lock_round)
            .append(&self.lock_votes)
            .append(&content);
    }
}

impl<T: Codec> Decodable for WalLock<T> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let lock_round: u64 = r.val_at(0)?;
                let lock_votes: AggregatedVote = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let content = Codec::decode(Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(WalLock {
                    lock_round,
                    lock_votes,
                    content,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl<T: Codec> Encodable for WalInfo<T> {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(5)
            .append(&self.height)
            .append(&self.round)
            .append::<u8>(&self.step.clone().into())
            .append(&self.lock)
            .append(&self.from);
    }
}

impl<T: Codec> Decodable for WalInfo<T> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(5) => {
                let height: u64 = r.val_at(0)?;
                let round: u64 = r.val_at(1)?;
                let tmp: u8 = r.val_at(2)?;
                let step = Step::from(tmp);
                let lock = r.val_at(3)?;
                let from: UpdateFrom = r.val_at(4)?;
                Ok(WalInfo {
                    height,
                    round,
                    step,
                    lock,
                    from,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl Encodable for Choke {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3)
            .append(&self.height)
            .append(&self.round)
            .append(&self.from);
    }
}

impl Decodable for Choke {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let height: u64 = r.val_at(0)?;
                let round: u64 = r.val_at(1)?;
                let from: UpdateFrom = r.val_at(2)?;
                Ok(Choke {
                    height,
                    round,
                    from,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl Encodable for SignedChoke {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3)
            .append(&self.signature.to_vec())
            .append(&self.choke)
            .append(&self.address.to_vec());
    }
}

impl Decodable for SignedChoke {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let signature = Signature::from(tmp);
                let choke: Choke = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let address = Address::from(tmp);
                Ok(SignedChoke {
                    signature,
                    choke,
                    address,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl Encodable for AggregatedChoke {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(4)
            .append(&self.height)
            .append(&self.round)
            .append(&self.signature.to_vec());
        let tmp: Vec<Vec<u8>> = self
            .voters
            .iter()
            .map(|addr| addr.to_vec())
            .collect::<Vec<_>>();
        s.append_list::<Vec<u8>, _>(&tmp);
    }
}

impl Decodable for AggregatedChoke {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let height: u64 = r.val_at(0)?;
                let round: u64 = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let signature = Signature::from(tmp);
                let tmp: Vec<Vec<u8>> = r.list_at(3)?;
                let voters = tmp.into_iter().map(Address::from).collect::<Vec<_>>();
                Ok(AggregatedChoke {
                    height,
                    round,
                    signature,
                    voters,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl Encodable for HashChoke {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2).append(&self.height).append(&self.round);
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use bincode::{deserialize, serialize};
    use rand::random;
    use serde::{Deserialize, Serialize};

    use super::*;

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

    impl<T: Codec> SignedProposal<T> {
        fn new(content: T, lock: Option<PoLC>) -> Self {
            SignedProposal {
                signature: gen_signature(),
                proposal:  Proposal::new(content, lock),
            }
        }
    }

    impl<T: Codec> Proposal<T> {
        fn new(content: T, lock: Option<PoLC>) -> Self {
            let height = random::<u64>();
            let round = random::<u64>();
            let block_hash = gen_hash();
            let proposer = gen_address();
            Proposal {
                height,
                round,
                content,
                block_hash,
                lock,
                proposer,
            }
        }
    }

    impl PoLC {
        fn new() -> Self {
            PoLC {
                lock_round: random::<u64>(),
                lock_votes: AggregatedVote::new(1u8),
            }
        }
    }

    impl SignedVote {
        fn new(vote_type: u8) -> Self {
            SignedVote {
                signature: gen_signature(),
                vote:      Vote::new(vote_type),
                voter:     gen_address(),
            }
        }
    }

    impl AggregatedVote {
        fn new(vote_type: u8) -> Self {
            AggregatedVote {
                signature:  gen_aggr_signature(),
                vote_type:  VoteType::try_from(vote_type).unwrap(),
                height:     random::<u64>(),
                round:      random::<u64>(),
                block_hash: gen_hash(),
                leader:     gen_address(),
            }
        }
    }

    impl Vote {
        fn new(vote_type: u8) -> Self {
            Vote {
                height:     random::<u64>(),
                round:      random::<u64>(),
                vote_type:  VoteType::try_from(vote_type).unwrap(),
                block_hash: gen_hash(),
            }
        }
    }

    impl<T: Codec> Commit<T> {
        fn new(content: T) -> Self {
            let height = random::<u64>();
            let proof = Proof::new();
            Commit {
                height,
                content,
                proof,
            }
        }
    }

    impl Proof {
        fn new() -> Self {
            Proof {
                height:     random::<u64>(),
                round:      random::<u64>(),
                block_hash: gen_hash(),
                signature:  gen_aggr_signature(),
            }
        }
    }

    impl AggregatedChoke {
        fn new() -> Self {
            AggregatedChoke {
                height:    random::<u64>(),
                round:     random::<u64>(),
                signature: gen_signature(),
                voters:    vec![gen_address(), gen_address()],
            }
        }
    }

    impl Choke {
        fn new(from: UpdateFrom) -> Self {
            Choke {
                height: random::<u64>(),
                round: random::<u64>(),
                from,
            }
        }
    }

    impl SignedChoke {
        fn new(from: UpdateFrom) -> Self {
            SignedChoke {
                signature: gen_signature(),
                address:   gen_address(),
                choke:     Choke::new(from),
            }
        }
    }

    impl Status {
        fn new(time: Option<u64>, is_update_config: bool) -> Self {
            let config = if is_update_config {
                Some(DurationConfig {
                    propose_ratio:   random::<u64>(),
                    prevote_ratio:   random::<u64>(),
                    precommit_ratio: random::<u64>(),
                    brake_ratio:     random::<u64>(),
                })
            } else {
                None
            };

            Status {
                height:         random::<u64>(),
                interval:       time,
                timer_config:   config,
                authority_list: vec![Node::new(gen_address())],
            }
        }
    }

    impl<T: Codec> WalInfo<T> {
        fn new(content: Option<T>) -> Self {
            let lock = if let Some(tmp) = content {
                let polc = PoLC::new();
                Some(WalLock {
                    lock_round: polc.lock_round,
                    lock_votes: polc.lock_votes,
                    content:    tmp,
                })
            } else {
                None
            };

            let height = random::<u64>();
            let round = random::<u64>();
            let step = Step::Precommit;
            let from = UpdateFrom::ChokeQC(AggregatedChoke::new());
            WalInfo {
                height,
                round,
                step,
                lock,
                from,
            }
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

    #[test]
    fn test_pill_codec() {
        for _ in 0..100 {
            let pill = Pill::new();
            let decode: Pill = Codec::decode(Codec::encode(&pill).unwrap()).unwrap();
            assert_eq!(decode, pill);
        }
    }

    #[test]
    fn test_types_rlp() {
        // Test SignedProposal
        let signed_proposal = SignedProposal::new(Pill::new(), Some(PoLC::new()));
        let res: SignedProposal<Pill> = rlp::decode(&signed_proposal.rlp_bytes()).unwrap();
        assert_eq!(signed_proposal, res);

        let signed_proposal = SignedProposal::new(Pill::new(), None);
        let res: SignedProposal<Pill> = rlp::decode(&signed_proposal.rlp_bytes()).unwrap();
        assert_eq!(signed_proposal, res);

        // Test SignedVote
        let signed_vote = SignedVote::new(2u8);
        let res: SignedVote = rlp::decode(&signed_vote.rlp_bytes()).unwrap();
        assert_eq!(signed_vote, res);

        let signed_vote = SignedVote::new(1u8);
        let res: SignedVote = rlp::decode(&signed_vote.rlp_bytes()).unwrap();
        assert_eq!(signed_vote, res);

        // Test AggregatedVote
        let aggregated_vote = AggregatedVote::new(2u8);
        let res: AggregatedVote = rlp::decode(&aggregated_vote.rlp_bytes()).unwrap();
        assert_eq!(aggregated_vote, res);

        let aggregated_vote = AggregatedVote::new(1u8);
        let res: AggregatedVote = rlp::decode(&aggregated_vote.rlp_bytes()).unwrap();
        assert_eq!(aggregated_vote, res);

        // Test Commit
        let commit = Commit::new(Pill::new());
        let res: Commit<Pill> = rlp::decode(&commit.rlp_bytes()).unwrap();
        assert_eq!(commit, res);

        // Test Status
        let status = Status::new(None, true);
        let res: Status = rlp::decode(&status.rlp_bytes()).unwrap();
        assert_eq!(status, res);

        // Test Status
        let status = Status::new(Some(3000), false);
        let res: Status = rlp::decode(&status.rlp_bytes()).unwrap();
        assert_eq!(status, res);

        // Test Aggregated Choke
        let aggregated_choke = AggregatedChoke::new();
        let res: AggregatedChoke = rlp::decode(&aggregated_choke.rlp_bytes()).unwrap();
        assert_eq!(aggregated_choke, res);

        // Test Signed Choke
        let signed_choke = SignedChoke::new(UpdateFrom::PrevoteQC(AggregatedVote::new(1u8)));
        let res: SignedChoke = rlp::decode(&signed_choke.rlp_bytes()).unwrap();
        assert_eq!(signed_choke, res);

        let signed_choke = SignedChoke::new(UpdateFrom::PrecommitQC(AggregatedVote::new(2u8)));
        let res: SignedChoke = rlp::decode(&signed_choke.rlp_bytes()).unwrap();
        assert_eq!(signed_choke, res);

        let signed_choke = SignedChoke::new(UpdateFrom::ChokeQC(AggregatedChoke::new()));
        let res: SignedChoke = rlp::decode(&signed_choke.rlp_bytes()).unwrap();
        assert_eq!(signed_choke, res);

        // Test Wal Info
        let pill = Pill::new();
        let wal_info = WalInfo::new(Some(pill));
        let res: WalInfo<Pill> = rlp::decode(&wal_info.rlp_bytes()).unwrap();
        assert_eq!(wal_info, res);

        let wal_info = WalInfo::new(None);
        let res: WalInfo<Pill> = rlp::decode(&wal_info.rlp_bytes()).unwrap();
        assert_eq!(wal_info, res);
    }
}
