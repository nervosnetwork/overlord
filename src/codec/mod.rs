use bytes::Bytes;
use rlp::{Decodable, DecoderError, Encodable, Prototype, Rlp, RlpStream};

use crate::state::{Stage, StateInfo, Step};
use crate::types::{
    Aggregates, Choke, ChokeQC, FetchedFullBlock, FullBlockWithProof, HeightRange, PreCommitQC,
    PreVoteQC, Proposal, SignedChoke, SignedHeight, SignedPreCommit, SignedPreVote, SignedProposal,
    SyncRequest, SyncResponse, UpdateFrom, Vote, Weight,
};
use crate::{Address, Blk, FullBlk, Hash, Height, Proof, Round, Signature};

// impl Encodable and Decodable trait for Aggregates
impl Encodable for Aggregates {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2)
            .append(&self.signature.to_vec())
            .append(&self.address_bitmap.to_vec());
    }
}

impl Decodable for Aggregates {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let signature = Signature::from(tmp);
                let tmp: Vec<u8> = r.val_at(1)?;
                let address_bitmap = Bytes::from(tmp);
                Ok(Aggregates {
                    signature,
                    address_bitmap,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Vote
impl Encodable for Vote {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3)
            .append(&self.height)
            .append(&self.round)
            .append(&self.block_hash.to_vec());
    }
}

impl Decodable for Vote {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let height: Height = r.val_at(0)?;
                let round: Round = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let block_hash = Hash::from(tmp);
                Ok(Vote {
                    height,
                    round,
                    block_hash,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for SignedPreVote
impl Encodable for SignedPreVote {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(4)
            .append(&self.signature.to_vec())
            .append(&self.vote)
            .append(&self.vote_weight)
            .append(&self.voter.to_vec());
    }
}

impl Decodable for SignedPreVote {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let signature = Signature::from(tmp);
                let vote: Vote = r.val_at(1)?;
                let vote_weight: Weight = r.val_at(2)?;
                let tmp: Vec<u8> = r.val_at(3)?;
                let voter = Address::from(tmp);
                Ok(SignedPreVote {
                    signature,
                    vote,
                    vote_weight,
                    voter,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for SignedPreCommit
impl Encodable for SignedPreCommit {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(4)
            .append(&self.signature.to_vec())
            .append(&self.vote)
            .append(&self.vote_weight)
            .append(&self.voter.to_vec());
    }
}

impl Decodable for SignedPreCommit {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let signature = Signature::from(tmp);
                let vote: Vote = r.val_at(1)?;
                let vote_weight: Weight = r.val_at(2)?;
                let tmp: Vec<u8> = r.val_at(3)?;
                let voter = Address::from(tmp);
                Ok(SignedPreCommit {
                    signature,
                    vote,
                    vote_weight,
                    voter,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for PreVoteQC
impl Encodable for PreVoteQC {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3)
            .append(&self.aggregates)
            .append(&self.vote)
            .append(&self.sender.to_vec());
    }
}

impl Decodable for PreVoteQC {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let aggregates: Aggregates = r.val_at(0)?;
                let vote: Vote = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let sender = Address::from(tmp);
                Ok(PreVoteQC {
                    aggregates,
                    vote,
                    sender,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for PreCommitQC
impl Encodable for PreCommitQC {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3)
            .append(&self.aggregates)
            .append(&self.vote)
            .append(&self.sender.to_vec());
    }
}

impl Decodable for PreCommitQC {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let aggregates: Aggregates = r.val_at(0)?;
                let vote: Vote = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let sender = Address::from(tmp);
                Ok(PreCommitQC {
                    aggregates,
                    vote,
                    sender,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Choke
impl Encodable for Choke {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2).append(&self.height).append(&self.round);
    }
}

impl Decodable for Choke {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let height: Height = r.val_at(0)?;
                let round: Round = r.val_at(1)?;
                Ok(Choke { height, round })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for UpdateFrom
impl Encodable for UpdateFrom {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2);
        match self {
            UpdateFrom::PreVoteQC(qc) => {
                s.append(&0u8).append(qc);
            }
            UpdateFrom::PreCommitQC(qc) => {
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
                        let qc: PreVoteQC = r.val_at(1)?;
                        UpdateFrom::PreVoteQC(qc)
                    }
                    1u8 => {
                        let qc: PreCommitQC = r.val_at(1)?;
                        UpdateFrom::PreCommitQC(qc)
                    }
                    2u8 => {
                        let qc: ChokeQC = r.val_at(1)?;
                        UpdateFrom::ChokeQC(qc)
                    }
                    _ => return Err(DecoderError::Custom("out of UpdateFrom's type range")),
                };
                Ok(res)
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for SignedChoke
impl Encodable for SignedChoke {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(5)
            .append(&self.choke)
            .append(&self.vote_weight)
            .append(&self.from)
            .append(&self.voter.to_vec())
            .append(&self.signature.to_vec());
    }
}

impl Decodable for SignedChoke {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(5) => {
                let choke: Choke = r.val_at(0)?;
                let vote_weight: Weight = r.val_at(1)?;
                let from: Option<UpdateFrom> = r.val_at(2)?;
                let tmp: Vec<u8> = r.val_at(3)?;
                let voter = Address::from(tmp);
                let tmp: Vec<u8> = r.val_at(4)?;
                let signature = Signature::from(tmp);
                Ok(SignedChoke {
                    choke,
                    vote_weight,
                    from,
                    voter,
                    signature,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for ChokeQC
impl Encodable for ChokeQC {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3)
            .append(&self.aggregates)
            .append(&self.choke)
            .append(&self.sender.to_vec());
    }
}

impl Decodable for ChokeQC {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let aggregates: Aggregates = r.val_at(0)?;
                let choke: Choke = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let sender = Address::from(tmp);
                Ok(ChokeQC {
                    aggregates,
                    choke,
                    sender,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Proposal
impl<B: Blk> Encodable for Proposal<B> {
    fn rlp_append(&self, s: &mut RlpStream) {
        let block = self.block.fixed_encode().unwrap().to_vec();
        s.begin_list(6)
            .append(&self.height)
            .append(&self.round)
            .append(&self.block_hash.to_vec())
            .append(&self.lock)
            .append(&self.proposer.to_vec())
            .append(&block);
    }
}

impl<B: Blk> Decodable for Proposal<B> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(6) => {
                let height: Height = r.val_at(0)?;
                let round: Round = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let block_hash = Hash::from(tmp);
                let lock: Option<PreVoteQC> = r.val_at(3)?;
                let tmp: Vec<u8> = r.val_at(4)?;
                let proposer: Address = Address::from(tmp);
                let tmp: Vec<u8> = r.val_at(5)?;
                let block: B = B::fixed_decode(&Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(Proposal {
                    height,
                    round,
                    block_hash,
                    lock,
                    proposer,
                    block,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for SignedProposal
impl<B: Blk> Encodable for SignedProposal<B> {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2)
            .append(&self.signature.to_vec())
            .append(&self.proposal);
    }
}

impl<B: Blk> Decodable for SignedProposal<B> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let tmp: Vec<u8> = r.val_at(0)?;
                let signature = Signature::from(tmp);
                let proposal: Proposal<B> = r.val_at(1)?;
                Ok(SignedProposal {
                    signature,
                    proposal,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for StateInfo
impl Encodable for Stage {
    fn rlp_append(&self, s: &mut RlpStream) {
        let step: u8 = self.step.clone().into();
        s.begin_list(3)
            .append(&self.height)
            .append(&self.round)
            .append(&step);
    }
}

impl Decodable for Stage {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let height: Height = r.val_at(0)?;
                let round: Round = r.val_at(1)?;
                let tmp: u8 = r.val_at(2)?;
                let step = Step::from(tmp);
                Ok(Stage {
                    height,
                    round,
                    step,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for StateInfo
impl<B: Blk> Encodable for StateInfo<B> {
    fn rlp_append(&self, s: &mut RlpStream) {
        let block = self
            .block
            .clone()
            .map(|block| block.fixed_encode().unwrap().to_vec());
        s.begin_list(6)
            .append(&self.stage)
            .append(&self.lock)
            .append(&self.block_hash.clone().map(|hash| hash.to_vec()))
            .append(&self.pre_commit_qc)
            .append(&self.from)
            .append(&block);
    }
}

impl<B: Blk> Decodable for StateInfo<B> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(6) => {
                let stage: Stage = r.val_at(0)?;
                let lock: Option<PreVoteQC> = r.val_at(1)?;
                let tmp: Option<Vec<u8>> = r.val_at(2)?;
                let block_hash = tmp.map(Hash::from);
                let pre_commit_qc: Option<PreCommitQC> = r.val_at(3)?;
                let from: Option<UpdateFrom> = r.val_at(4)?;
                let tmp: Option<Vec<u8>> = r.val_at(5)?;
                let block = if let Some(v) = tmp {
                    Some(
                        B::fixed_decode(&Bytes::from(v))
                            .map_err(|_| DecoderError::Custom("Codec decode error."))?,
                    )
                } else {
                    None
                };
                Ok(StateInfo {
                    stage,
                    lock,
                    block_hash,
                    pre_commit_qc,
                    from,
                    block,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for FetchedFullBlock
impl<B: Blk, F: FullBlk<B>> Encodable for FetchedFullBlock<B, F> {
    fn rlp_append(&self, s: &mut RlpStream) {
        let full_block = self.full_block.fixed_encode().unwrap().to_vec();
        s.begin_list(3)
            .append(&self.height)
            .append(&self.block_hash.to_vec())
            .append(&full_block);
    }
}

impl<B: Blk, F: FullBlk<B>> Decodable for FetchedFullBlock<B, F> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let height: Height = r.val_at(0)?;
                let tmp: Vec<u8> = r.val_at(1)?;
                let block_hash = Hash::from(tmp);
                let tmp: Vec<u8> = r.val_at(2)?;
                let full_block: F = F::fixed_decode(&Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(FetchedFullBlock::new(height, block_hash, full_block))
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for HeightRange
impl Encodable for HeightRange {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2).append(&self.from).append(&self.to);
    }
}

impl Decodable for HeightRange {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let from = r.val_at(0)?;
                let to = r.val_at(1)?;
                Ok(HeightRange { from, to })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for SignedHeight
impl Encodable for SignedHeight {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(4)
            .append(&self.height)
            .append(&self.address.to_vec())
            .append(&self.pub_key_hex)
            .append(&self.signature.to_vec());
    }
}

impl Decodable for SignedHeight {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let height: Height = r.val_at(0)?;
                let tmp: Vec<u8> = r.val_at(1)?;
                let address = Address::from(tmp);
                let pub_key_hex: String = r.val_at(2)?;
                let tmp: Vec<u8> = r.val_at(3)?;
                let signature = Signature::from(tmp);
                Ok(SignedHeight {
                    height,
                    address,
                    pub_key_hex,
                    signature,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for SyncRequest
impl Encodable for SyncRequest {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(4)
            .append(&self.request_range)
            .append(&self.requester.to_vec())
            .append(&self.pub_key_hex)
            .append(&self.signature.to_vec());
    }
}

impl Decodable for SyncRequest {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(4) => {
                let request_range: HeightRange = r.val_at(0)?;
                let tmp: Vec<u8> = r.val_at(1)?;
                let requester = Address::from(tmp);
                let pub_key_hex: String = r.val_at(2)?;
                let tmp: Vec<u8> = r.val_at(3)?;
                let signature = Signature::from(tmp);
                Ok(SyncRequest {
                    request_range,
                    requester,
                    pub_key_hex,
                    signature,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for SyncRequest
impl<B: Blk, F: FullBlk<B>> Encodable for SyncResponse<B, F> {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(5)
            .append(&self.request_range)
            .append(&self.responder.to_vec())
            .append(&self.pub_key_hex)
            .append(&self.signature.to_vec())
            .append_list(&self.block_with_proofs);
    }
}

impl<B: Blk, F: FullBlk<B>> Decodable for SyncResponse<B, F> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(5) => {
                let request_range: HeightRange = r.val_at(0)?;
                let tmp: Vec<u8> = r.val_at(1)?;
                let responder = Address::from(tmp);
                let pub_key_hex: String = r.val_at(2)?;
                let tmp: Vec<u8> = r.val_at(3)?;
                let signature = Signature::from(tmp);
                let block_with_proofs: Vec<FullBlockWithProof<B, F>> = r.list_at(4)?;
                Ok(SyncResponse {
                    request_range,
                    responder,
                    pub_key_hex,
                    signature,
                    block_with_proofs,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl<B: Blk, F: FullBlk<B>> Encodable for FullBlockWithProof<B, F> {
    fn rlp_append(&self, s: &mut RlpStream) {
        let full_block = self.full_block.fixed_encode().unwrap().to_vec();
        s.begin_list(2).append(&self.proof).append(&full_block);
    }
}

impl<B: Blk, F: FullBlk<B>> Decodable for FullBlockWithProof<B, F> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let proof: Proof = r.val_at(0)?;
                let tmp: Vec<u8> = r.val_at(1)?;
                let full_block: F = F::fixed_decode(&Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(FullBlockWithProof::new(proof, full_block))
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::{TestBlock, TestFullBlock};
    use std::fmt::Debug;

    #[test]
    fn test_codec() {
        test_rlp::<Stage>();
        test_rlp::<StateInfo<TestBlock>>();
        test_rlp::<Aggregates>();
        test_rlp::<Choke>();
        test_rlp::<ChokeQC>();
        test_rlp::<FetchedFullBlock<TestBlock, TestFullBlock>>();
        test_rlp::<PreCommitQC>();
        test_rlp::<PreVoteQC>();
        test_rlp::<Proposal<TestBlock>>();
        test_rlp::<SignedChoke>();
        test_rlp::<SignedPreCommit>();
        test_rlp::<SignedPreVote>();
        test_rlp::<SignedProposal<TestBlock>>();
        test_rlp::<UpdateFrom>();
        test_rlp::<Vote>();
        test_rlp::<ChokeQC>();
        test_rlp::<HeightRange>();
        test_rlp::<SignedHeight>();
        test_rlp::<SyncRequest>();
        test_rlp::<SyncResponse<TestBlock, TestFullBlock>>();
    }

    fn test_rlp<T: Debug + Default + PartialEq + Eq + Decodable + Encodable>() {
        let data = T::default();
        let encode = rlp::encode(&data);
        let decode = rlp::decode(&encode).unwrap();
        assert_eq!(data, decode);
    }
}
