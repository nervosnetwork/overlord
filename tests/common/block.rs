use std::error::Error;

use bytes::Bytes;
use overlord::{Blk, Crypto, DefaultCrypto, Hash, Height, Proof};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pre_hash:      Hash,
    height:        Height,
    exec_height:   Height,
    proof:         Proof,
    state_root:    Hash,
    receipt_roots: Vec<Hash>,
    tx_root:       Hash,
}

#[derive(Clone, Debug, Default)]
pub struct ExecState {
    state_root:   Hash,
    receipt_root: Hash,
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
        self.proof.clone()
    }
}

#[test]
fn test_block_serialization() {
    let block = Block::default();
    println! {"{:?}", block};
    let encode = block.encode().unwrap();
    let decode = Block::decode(&encode).unwrap();
    assert_eq!(decode, block);
}
