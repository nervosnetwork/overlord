use bytes::Bytes;
use rlp::{Decodable, DecoderError, Encodable, Prototype, Rlp, RlpStream};

use crate::state::{Stage, StateInfo, Step};
use crate::types::{
    Aggregates, Choke, ChokeQC, PoLC, PreCommitQC, PreVoteQC, Proposal, SignedChoke,
    SignedPreCommit, SignedPreVote, SignedProposal, TimeConfig, UpdateFrom, Vote, Weight,
};
use crate::{Address, Blk, Hash, Height, Proof, Round, Signature};

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
        s.begin_list(2).append(&self.aggregates).append(&self.vote);
    }
}

impl Decodable for PreVoteQC {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let aggregates: Aggregates = r.val_at(0)?;
                let vote: Vote = r.val_at(1)?;
                Ok(PreVoteQC { aggregates, vote })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for PreCommitQC
impl Encodable for PreCommitQC {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2).append(&self.aggregates).append(&self.vote);
    }
}

impl Decodable for PreCommitQC {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let aggregates: Aggregates = r.val_at(0)?;
                let vote: Vote = r.val_at(1)?;
                Ok(PreCommitQC { aggregates, vote })
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
                let from: UpdateFrom = r.val_at(2)?;
                let tmp: Vec<u8> = r.val_at(3)?;
                let voter = Address::from(tmp);
                let tmp: Vec<u8> = r.val_at(3)?;
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
        s.begin_list(2).append(&self.aggregates).append(&self.choke);
    }
}

impl Decodable for ChokeQC {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let aggregates: Aggregates = r.val_at(0)?;
                let choke: Choke = r.val_at(1)?;
                Ok(ChokeQC { aggregates, choke })
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
                let lock_round: Round = r.val_at(0)?;
                let lock_votes: PreVoteQC = r.val_at(1)?;
                Ok(PoLC {
                    lock_round,
                    lock_votes,
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
                let lock: Option<PoLC> = r.val_at(3)?;
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
            .as_ref()
            .map(|block| block.fixed_encode().unwrap().to_vec());
        s.begin_list(9)
            .append(&self.stage)
            .append(&self.polc)
            .append(&self.pre_commit_qc)
            .append(&self.pre_proof)
            .append(&self.from)
            .append(&self.time_config)
            .append(&self.pre_hash.to_vec())
            .append(&self.pre_exec_height)
            .append(&block);
    }
}

impl<B: Blk> Decodable for StateInfo<B> {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(9) => {
                let stage: Stage = r.val_at(0)?;
                let polc: Option<PoLC> = r.val_at(1)?;
                let pre_commit_qc: Option<PreCommitQC> = r.val_at(2)?;
                let pre_proof: Proof = r.val_at(3)?;
                let from: Option<UpdateFrom> = r.val_at(4)?;
                let time_config: TimeConfig = r.val_at(5)?;
                let tmp: Vec<u8> = r.val_at(6)?;
                let pre_hash = Hash::from(tmp);
                let pre_exec_height: Height = r.val_at(7)?;
                let tmp: Option<Vec<u8>> = r.val_at(8)?;
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
                    time_config,
                    polc,
                    pre_commit_qc,
                    pre_proof,
                    pre_hash,
                    pre_exec_height,
                    from,
                    block,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for Proof
impl Encodable for Proof {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2).append(&self.vote).append(&self.aggregates);
    }
}

impl Decodable for Proof {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(2) => {
                let vote: Vote = r.val_at(0)?;
                let aggregates: Aggregates = r.val_at(1)?;
                Ok(Proof { vote, aggregates })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

// impl Encodable and Decodable trait for TimeConfig
impl Encodable for TimeConfig {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(5)
            .append(&self.interval)
            .append(&self.propose_ratio)
            .append(&self.pre_vote_ratio)
            .append(&self.pre_commit_ratio)
            .append(&self.brake_ratio);
    }
}

impl Decodable for TimeConfig {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(5) => {
                let interval: u64 = r.val_at(0)?;
                let propose_ratio: u64 = r.val_at(1)?;
                let pre_vote_ratio: u64 = r.val_at(2)?;
                let pre_commit_ratio: u64 = r.val_at(3)?;
                let brake_ratio: u64 = r.val_at(4)?;
                Ok(TimeConfig {
                    interval,
                    propose_ratio,
                    pre_vote_ratio,
                    pre_commit_ratio,
                    brake_ratio,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}
