use std::error::Error;

use bytes::Bytes;
use derive_more::Display;

use crate::types::{SignedProposal, Proposal, PoLC, Vote, SignedPreVote, SignedPreCommit, PreVoteQC,
PreCommitQC, ChokeQC, Aggregates, SignedChoke, UpdateFrom, };

pub trait FixedCodec: Sized {
    fn encode_fixed(&self) -> ProtocolResult<Bytes>;

    fn decode_fixed(bytes: Bytes) -> ProtocolResult<Self>;
}

#[macro_export]
macro_rules! impl_default_fixed_codec_for {
    ($($type:ident),+) => (
        $(
            impl FixedCodec for crate::types::$type {
                fn encode_fixed(&self) -> Result<bytes::Bytes, FixedCodecError> {
                    Ok(bytes::Bytes::from(rlp::encode(self)))
                }

                fn decode_fixed(bytes: bytes::Bytes) -> Result<Self, FixedCodecError> {
                    Ok(rlp::decode(bytes.as_ref()).map_err(FixedCodecError::Decode)?)
                }
            }
        )+
    )
}

impl rlp::Encodable for Proof {
    fn rlp_append(&self, s: &mut rlp::RlpStream) {
        s.begin_list(5)
            .append(&self.bitmap.to_vec())
            .append(&self.block_hash)
            .append(&self.height)
            .append(&self.round)
            .append(&self.signature.to_vec());
    }
}

impl rlp::Decodable for Proof {
    fn decode(r: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        if !r.is_list() && r.size() != 5 {
            return Err(rlp::DecoderError::RlpIncorrectListLen);
        }

        let bitmap = BytesMut::from(r.at(0)?.data()?).freeze();
        let block_hash: Hash = rlp::decode(r.at(1)?.as_raw())?;
        let height = r.at(2)?.as_val()?;
        let round = r.at(3)?.as_val()?;
        let signature = BytesMut::from(r.at(4)?.data()?).freeze();

        Ok(Proof {
            height,
            round,
            block_hash,
            signature,
            bitmap,
        })
    }
}



#[derive(Debug, Display, From)]
pub enum FixedCodecError {
    #[display(fmt = "Fix decode error {:?}", _0)]
    Decoder(rlp::DecoderError),
    #[display(fmt = "From utf-8 error {:?}", _0)]
    StringUTF8(std::string::FromUtf8Error),
    // #[display(fmt = "wrong bytes of bool")]
    // DecodeBool,
    // #[display(fmt = "wrong bytes of u8")]
    // DecodeUint8,
}

impl Error for FixedCodecError {}