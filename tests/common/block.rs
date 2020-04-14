use std::error::Error;

use bytes::Bytes;
use derive_more::Display;
use overlord::{Blk, Crypto, DefaultCrypto, Hash, Height, OverlordConfig, Proof, St};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ pre_hash: {}, height: {}, exec_height: {}, pre_proof: {}, state_root: {}, tx: {} }}",
    "hex::encode(pre_hash)",
    height,
    exec_height,
    pre_proof,
    "hex::encode(state_root)",
    tx
)]
pub struct Block {
    pub pre_hash:      Hash,
    pub height:        Height,
    pub exec_height:   Height,
    pub pre_proof:     Proof,
    pub state_root:    Hash,
    pub receipt_roots: Vec<Hash>,
    pub tx:            Transaction,
}

impl Block {
    pub fn genesis_block() -> Self {
        Block::default()
    }
}

impl Blk for Block {
    fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
        Ok(bincode::serialize(self).map(Bytes::from).unwrap())
    }

    fn decode(data: &Bytes) -> Result<Self, Box<dyn Error + Send>> {
        Ok(bincode::deserialize(data.as_ref()).unwrap())
    }

    fn get_block_hash(&self) -> Hash {
        DefaultCrypto::hash(&self.encode().unwrap())
    }

    fn get_pre_hash(&self) -> Hash {
        self.pre_hash.clone()
    }

    fn get_height(&self) -> Height {
        self.height
    }

    fn get_exec_height(&self) -> Height {
        self.exec_height
    }

    fn get_proof(&self) -> Proof {
        self.pre_proof.clone()
    }
}

pub type Transaction = OverlordConfig;

#[derive(Clone, Debug, Default)]
pub struct ExecState {
    pub state_root:   Hash,
    pub receipt_root: Hash,
}

impl St for ExecState {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullBlock {
    pub block: Block,
}

#[test]
fn test_block_serialization() {
    let block = Block::default();
    println! {"{:?}", block};
    let encode = block.encode().unwrap();
    let decode = Block::decode(&encode).unwrap();
    assert_eq!(decode, block);
}
