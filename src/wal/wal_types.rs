use bytes::Bytes;
use rlp::{Decodable, DecoderError, Encodable, Prototype, Rlp, RlpStream};

use crate::smr::smr_types::Step;
use crate::types::{AggregatedSignature, Node, Proposal};
use crate::Codec;

pub enum WalMsg<P: Codec, C: Codec> {
    Authority(Vec<Node>),
    EpochId(u64),
    Round(u64),
    Step(Step),
    Proposal(Proposal<P>),
    FullTxs(C),
    QC(AggregatedSignature),
}

#[derive(Clone, Debug)]
pub struct WalStatus<P: Codec, C: Codec> {
    pub authority: Vec<Node>,
    pub epoch_id:  u64,
    pub round:     u64,
    pub step:      Step,
    pub proposal:  Option<Proposal<P>>,
    pub full_txs:  Option<C>,
    pub qc:        Option<AggregatedSignature>,
}

#[derive(Clone, Debug)]
pub struct ValidatorList(pub Vec<Node>);

impl Encodable for ValidatorList {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(1).append_list(&self.0);
    }
}

impl Decodable for ValidatorList {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(1) => {
                let validators: Vec<Node> = r.list_at(0)?;
                Ok(ValidatorList(validators))
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl ValidatorList {
    pub fn new(nodes: Vec<Node>) -> Self {
        ValidatorList(nodes)
    }
}

#[derive(Clone, Debug)]
pub struct WalEpochId(pub u64);

impl Encodable for WalEpochId {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(1).append(&self.0);
    }
}

impl Decodable for WalEpochId {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(1) => {
                let id: u64 = r.val_at(0)?;
                Ok(WalEpochId(id))
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl WalEpochId {
    pub fn new(id: u64) -> Self {
        WalEpochId(id)
    }
}

#[derive(Clone, Debug)]
pub struct WalRound(pub u64);

impl Encodable for WalRound {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(1).append(&self.0);
    }
}

impl Decodable for WalRound {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(1) => {
                let round: u64 = r.val_at(0)?;
                Ok(WalRound(round))
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl WalRound {
    pub fn new(round: u64) -> Self {
        WalRound(round)
    }
}

#[derive(Clone, Debug)]
pub struct WalStep(u8);

impl From<Step> for WalStep {
    fn from(step: Step) -> Self {
        let s: u8 = match step {
            Step::Propose => 0,
            Step::Prevote => 1,
            Step::Precommit => 2,
            Step::Commit => 3,
        };

        WalStep(s)
    }
}

impl Into<Step> for WalStep {
    fn into(self) -> Step {
        match self.0 {
            0u8 => Step::Propose,
            1u8 => Step::Prevote,
            2u8 => Step::Precommit,
            3u8 => Step::Commit,
            _ => panic!("Invalid wal step"),
        }
    }
}

impl Encodable for WalStep {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(1).append(&self.0);
    }
}

impl Decodable for WalStep {
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(1) => {
                let s: u8 = r.val_at(0)?;
                Ok(WalStep(s))
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Lock<P: Codec, C: Codec> {
    pub proposal: Proposal<P>,
    pub full_txs: C,
    pub qc:       AggregatedSignature,
}

impl<P, C> Encodable for Lock<P, C>
where
    P: Codec,
    C: Codec,
{
    fn rlp_append(&self, s: &mut RlpStream) {
        let content = self.full_txs.encode().unwrap().to_vec();
        s.begin_list(3)
            .append(&self.proposal)
            .append(&self.qc)
            .append(&content);
    }
}

impl<P, C> Decodable for Lock<P, C>
where
    P: Codec,
    C: Codec,
{
    fn decode(r: &Rlp) -> Result<Self, DecoderError> {
        match r.prototype()? {
            Prototype::List(3) => {
                let proposal: Proposal<P> = r.val_at(0)?;
                let qc: AggregatedSignature = r.val_at(1)?;
                let tmp: Vec<u8> = r.val_at(2)?;
                let full_txs = Codec::decode(Bytes::from(tmp))
                    .map_err(|_| DecoderError::Custom("Codec decode error."))?;
                Ok(Lock {
                    proposal,
                    full_txs,
                    qc,
                })
            }
            _ => Err(DecoderError::RlpInconsistentLengthAndData),
        }
    }
}

impl<P, C> Lock<P, C>
where
    P: Codec,
    C: Codec,
{
    pub fn new(proposal: Proposal<P>, full_txs: C, qc: AggregatedSignature) -> Self {
        Lock {
            proposal,
            full_txs,
            qc,
        }
    }
}
