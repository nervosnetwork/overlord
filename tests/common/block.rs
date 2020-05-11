use std::error::Error;

use bytes::Bytes;
use derive_more::Display;
use overlord::crypto::{hex_to_address, KeyPairs};
use overlord::types::SelectMode;
use overlord::{
    AuthConfig, Blk, Crypto, DefaultCrypto, FullBlk, Hash, Height, Node, OverlordConfig, Proof, St,
    TimeConfig, TinyHex,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    fmt = "{{ pre_hash: {}, height: {}, exec_height: {}, state_root: {}, receipt_roots: {:?}, pre_proof: {}, tx: {} }}",
    "pre_hash.tiny_hex()",
    height,
    exec_height,
    "state_root.tiny_hex()",
    "receipt_roots.iter().map(|hash| hash.tiny_hex()).collect::<Vec<String>>()",
    pre_proof,
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
    pub nonce:         Hash,
}

impl Block {
    pub fn new(
        key_pairs: &KeyPairs,
        mode: SelectMode,
        max_exec_behind: u64,
        time_config: TimeConfig,
    ) -> Self {
        let common_ref = key_pairs.common_ref.clone();

        let auth_list = key_pairs
            .key_pairs
            .iter()
            .map(|key_pair| {
                Node::new(
                    hex_to_address(&key_pair.address).unwrap(),
                    key_pair.bls_public_key.clone(),
                    1,
                    1,
                )
            })
            .collect();
        let auth_config = AuthConfig {
            common_ref,
            mode,
            auth_list,
        };
        let overlord_config = OverlordConfig {
            max_exec_behind,
            auth_config,
            time_config,
        };
        let mut block = Block::default();
        block.tx = overlord_config;
        block
    }
}

impl Blk for Block {
    fn fixed_encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
        Ok(bincode::serialize(self)
            .map(Bytes::from)
            .expect("serialize a block failed"))
    }

    fn fixed_decode(data: &Bytes) -> Result<Self, Box<dyn Error + Send>> {
        Ok(bincode::deserialize(data.as_ref()).expect("deserialize a block failed"))
    }

    fn get_block_hash(&self) -> Result<Hash, Box<dyn Error + Send>> {
        Ok(DefaultCrypto::hash(
            &self.fixed_encode().expect("fixed encode a block failed"),
        ))
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

#[derive(Clone, Debug, Display, Default)]
#[display(
    fmt = "{{ state_root: {}, receipt_root: {} }}",
    "state_root.tiny_hex()",
    "receipt_root.tiny_hex()"
)]
pub struct ExecState {
    pub state_root:   Hash,
    pub receipt_root: Hash,
}

impl St for ExecState {}

#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Serialize, Deserialize)]
#[display(fmt = "{{ block: {} }}", block)]
pub struct FullBlock {
    pub block: Block,
}

impl FullBlk<Block> for FullBlock {
    fn fixed_encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
        Ok(bincode::serialize(self)
            .map(Bytes::from)
            .expect("serialize a full block failed"))
    }

    fn fixed_decode(data: &Bytes) -> Result<Self, Box<dyn Error + Send>> {
        Ok(bincode::deserialize(data.as_ref()).expect("deserialize a full block failed"))
    }

    fn get_block(&self) -> Block {
        self.block.clone()
    }
}

#[test]
fn test_block_serialization() {
    let block = Block::default();
    let encode = block.fixed_encode().unwrap();
    let decode = Block::fixed_decode(&encode).unwrap();
    assert_eq!(decode, block);
}
