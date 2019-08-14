use bytes::Bytes;
use rlp::{Decodable, DecoderError, Encodable, Prototype, Rlp, RlpStream};

use crate::types::{
    Address, AggregatedSignature, AggregatedVote, Commit, Feed, Hash, Node, PoLC, Proof, Proposal,
    Signature, SignedProposal, SignedVote, Status, VerifyResp, Vote, VoteType,
};
use crate::Codec;

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
            .append(&self.epoch_id)
            .append(&self.round)
            .append(&self.epoch_hash.to_vec())
            .append(&self.lock)
            .append(&self.proposer.to_vec())
            .append(&content);
    }
}

impl<T: Codec> Decodable for Proposal<T> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(6) => {
                let epoch_id: u64 = r.val_at(0)?;
                let round: u64 = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let epoch_hash = Hash::from(tmp);
                let lock = r.val_at(3)?;
                let tmp: Vec<u8> = r.val_at(4)?;
                let proposer = Address::from(tmp);
                let tmp: Vec<u8> = r.val_at(5)?;
                let content = Codec::decode(Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(Proposal {
                    epoch_id,
                    round,
                    content,
                    epoch_hash,
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
        s.begin_list(5)
            .append(&self.signature)
            .append(&vote_type)
            .append(&self.epoch_id)
            .append(&self.round)
            .append(&self.epoch_hash.to_vec());
    }
}

impl Decodable for AggregatedVote {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(5) => {
                let signature: AggregatedSignature = r.val_at(0)?;
                let tmp: u8 = r.val_at(1)?;
                let vote_type = VoteType::from(tmp);
                let epoch_id: u64 = r.val_at(2)?;
                let round: u64 = r.val_at(3)?;
                let tmp: Vec<u8> = r.val_at(4)?;
                let epoch_hash = Hash::from(tmp);
                Ok(AggregatedVote {
                    signature,
                    vote_type,
                    epoch_id,
                    round,
                    epoch_hash,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for SignedVote
impl Encodable for SignedVote {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2)
            .append(&self.signature.to_vec())
            .append(&self.vote);
    }
}

impl Decodable for SignedVote {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let signature = Signature::from(tmp);
                let vote = r.val_at(1)?;
                Ok(SignedVote { signature, vote })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Vote
impl Encodable for Vote {
    fn rlp_append(&self, s: &mut RlpStream) {
        let vote_type: u8 = self.vote_type.clone().into();
        s.begin_list(5)
            .append(&self.epoch_id)
            .append(&self.round)
            .append(&vote_type)
            .append(&self.epoch_hash.to_vec())
            .append(&self.voter.to_vec());
    }
}

impl Decodable for Vote {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(5) => {
                let epoch_id: u64 = r.val_at(0)?;
                let round: u64 = r.val_at(1)?;
                let tmp: u8 = r.val_at(2)?;
                let vote_type = VoteType::from(tmp);
                let tmp: Vec<u8> = r.val_at(3)?;
                let epoch_hash = Hash::from(tmp);
                let tmp: Vec<u8> = r.val_at(4)?;
                let voter = Address::from(tmp);
                Ok(Vote {
                    epoch_id,
                    round,
                    vote_type,
                    epoch_hash,
                    voter,
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
            .append(&self.epoch_id)
            .append(&self.proof)
            .append(&content);
    }
}

impl<T: Codec> Decodable for Commit<T> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let epoch_id: u64 = r.val_at(0)?;
                let proof: Proof = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let content = Codec::decode(Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(Commit {
                    epoch_id,
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
            .append(&self.epoch_id)
            .append(&self.round)
            .append(&self.epoch_hash.to_vec())
            .append(&self.signature);
    }
}

impl Decodable for Proof {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let epoch_id: u64 = r.val_at(0)?;
                let round: u64 = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let epoch_hash = Hash::from(tmp);
                let signature: AggregatedSignature = r.val_at(3)?;
                Ok(Proof {
                    epoch_id,
                    round,
                    epoch_hash,
                    signature,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Status
impl Encodable for Status {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3)
            .append(&self.epoch_id)
            .append(&self.interval)
            .append_list(&self.authority_list);
    }
}

impl Decodable for Status {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let epoch_id: u64 = r.val_at(0)?;
                let interval: u64 = r.val_at(1)?;
                let authority_list: Vec<Node> = r.list_at(2)?;
                Ok(Status {
                    epoch_id,
                    interval,
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
            .append(&self.proposal_weight)
            .append(&self.vote_weight);
    }
}

impl Decodable for Node {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let address = Address::from(tmp);
                let proposal_weight: usize = r.val_at(1)?;
                let vote_weight: usize = r.val_at(2)?;
                Ok(Node {
                    address,
                    proposal_weight,
                    vote_weight,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Feed
impl<T: Codec> Encodable for Feed<T> {
    fn rlp_append(&self, s: &mut RlpStream) {
        let content = self.content.encode().unwrap().to_vec();
        s.begin_list(3)
            .append(&self.epoch_id)
            .append(&self.epoch_hash.to_vec())
            .append(&content);
    }
}

impl<T: Codec> Decodable for Feed<T> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let epoch_id: u64 = r.val_at(0)?;
                let tmp: Vec<u8> = r.val_at(1)?;
                let epoch_hash = Hash::from(tmp);
                let tmp: Vec<u8> = r.val_at(2)?;
                let content = Codec::decode(Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(Feed {
                    epoch_id,
                    epoch_hash,
                    content,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for VerifyResp
impl Encodable for VerifyResp {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2)
            .append(&self.epoch_hash.to_vec())
            .append(&self.is_pass);
    }
}

impl Decodable for VerifyResp {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let epoch_hash = Hash::from(tmp);
                let is_pass: bool = r.val_at(1)?;
                Ok(VerifyResp {
                    epoch_hash,
                    is_pass,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use bincode::{deserialize, serialize};
    use bytes::Bytes;
    use rand::random;
    use rlp::Encodable;
    use serde::{Deserialize, Serialize};

    use crate::types::{
        Address, AggregatedSignature, AggregatedVote, Commit, Feed, Hash, Node, PoLC, Proof,
        Proposal, Signature, SignedProposal, SignedVote, Status, VerifyResp, Vote, VoteType,
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
            let epoch_id = random::<u64>();
            let round = random::<u64>();
            let epoch_hash = gen_hash();
            let proposer = gen_address();
            Proposal {
                epoch_id,
                round,
                content,
                epoch_hash,
                lock,
                proposer,
            }
        }
    }

    impl PoLC {
        fn new() -> Self {
            PoLC {
                lock_round: random::<u64>(),
                lock_votes: AggregatedVote::new(0u8),
            }
        }
    }

    impl SignedVote {
        fn new(vote_type: u8) -> Self {
            SignedVote {
                signature: gen_signature(),
                vote:      Vote::new(vote_type),
            }
        }
    }

    impl AggregatedVote {
        fn new(vote_type: u8) -> Self {
            AggregatedVote {
                signature:  gen_aggr_signature(),
                vote_type:  VoteType::from(vote_type),
                epoch_id:   random::<u64>(),
                round:      random::<u64>(),
                epoch_hash: gen_hash(),
            }
        }
    }

    impl Vote {
        fn new(vote_type: u8) -> Self {
            Vote {
                epoch_id:   random::<u64>(),
                round:      random::<u64>(),
                vote_type:  VoteType::from(vote_type),
                epoch_hash: gen_hash(),
                voter:      gen_address(),
            }
        }
    }

    impl<T: Codec> Commit<T> {
        fn new(content: T) -> Self {
            let epoch_id = random::<u64>();
            let proof = Proof::new();
            Commit {
                epoch_id,
                content,
                proof,
            }
        }
    }

    impl Proof {
        fn new() -> Self {
            Proof {
                epoch_id:   random::<u64>(),
                round:      random::<u64>(),
                epoch_hash: gen_hash(),
                signature:  gen_aggr_signature(),
            }
        }
    }

    impl Status {
        fn new() -> Self {
            Status {
                epoch_id:       random::<u64>(),
                interval:       random::<u64>(),
                authority_list: vec![Node::new()],
            }
        }
    }

    impl Node {
        fn new() -> Self {
            Node {
                address:         gen_address(),
                proposal_weight: random::<usize>(),
                vote_weight:     random::<usize>(),
            }
        }
    }

    impl<T: Codec> Feed<T> {
        fn new(content: T) -> Self {
            let epoch_id = random::<u64>();
            let epoch_hash = gen_hash();
            Feed {
                epoch_id,
                content,
                epoch_hash,
            }
        }
    }

    impl VerifyResp {
        fn new(is_pass: bool) -> Self {
            let epoch_hash = gen_hash();
            VerifyResp {
                epoch_hash,
                is_pass,
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
        let signed_vote = SignedVote::new(0u8);
        let res: SignedVote = rlp::decode(&signed_vote.rlp_bytes()).unwrap();
        assert_eq!(signed_vote, res);

        let signed_vote = SignedVote::new(1u8);
        let res: SignedVote = rlp::decode(&signed_vote.rlp_bytes()).unwrap();
        assert_eq!(signed_vote, res);

        // Test AggregatedVote
        let aggregrated_vote = AggregatedVote::new(0u8);
        let res: AggregatedVote = rlp::decode(&aggregrated_vote.rlp_bytes()).unwrap();
        assert_eq!(aggregrated_vote, res);

        let aggregrated_vote = AggregatedVote::new(1u8);
        let res: AggregatedVote = rlp::decode(&aggregrated_vote.rlp_bytes()).unwrap();
        assert_eq!(aggregrated_vote, res);

        // Test Commit
        let commit = Commit::new(Pill::new());
        let res: Commit<Pill> = rlp::decode(&commit.rlp_bytes()).unwrap();
        assert_eq!(commit, res);

        // Test Status
        let status = Status::new();
        let res: Status = rlp::decode(&status.rlp_bytes()).unwrap();
        assert_eq!(status, res);

        // Test Feed
        let feed = Feed::new(Pill::new());
        let res: Feed<Pill> = rlp::decode(&feed.rlp_bytes()).unwrap();
        assert_eq!(feed, res);

        // Test VerifyResp
        let verify_response = VerifyResp::new(true);
        let res: VerifyResp = rlp::decode(&verify_response.rlp_bytes()).unwrap();
        assert_eq!(verify_response, res);

        let verify_response = VerifyResp::new(false);
        let res: VerifyResp = rlp::decode(&verify_response.rlp_bytes()).unwrap();
        assert_eq!(verify_response, res);
    }
}
