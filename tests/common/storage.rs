#![allow(dead_code)]

use std::collections::HashMap;

use overlord::{Address, Height, HeightRange, Proof, TinyHex};
use parking_lot::RwLock;

use crate::common::block::Block;

#[derive(Default)]
pub struct Storage {
    latest_height_map: RwLock<HashMap<Address, Height>>,
    block_map:         RwLock<HashMap<Height, Block>>,
    proof_map:         RwLock<HashMap<Height, Proof>>,
}

impl Storage {
    pub fn register(&self, address: Address) {
        let mut latest_height_map = self.latest_height_map.write();
        (*latest_height_map).insert(address, 0);
    }

    pub fn save_block_with_proof(&self, to: Address, height: Height, block: Block, proof: Proof) {
        self.ensure_right_height(&to, height);

        let mut block_map = self.block_map.write();
        if let Some(block_exists) = block_map.get(&height) {
            assert_eq!(
                block_exists,
                &block,
                "{} save a byzantine block {}",
                to.tiny_hex(),
                block
            );
        } else {
            let mut proof_map = self.proof_map.write();
            block_map.insert(height, block);
            proof_map.insert(height, proof);
        }
        let mut latest_height_map = self.latest_height_map.write();
        latest_height_map.insert(to, height);
    }

    pub fn get_block_with_proof(&self, from: &Address, range: HeightRange) -> Vec<(Block, Proof)> {
        let latest_height = self.get_latest_height(from);

        let height_start = range.from;
        let height_end = if range.to < latest_height {
            range.to
        } else {
            latest_height + 1
        };

        let block_map = self.block_map.read();
        let proof_map = self.proof_map.read();

        (height_start..height_end)
            .map(|height| {
                (
                    block_map.get(&height).unwrap().clone(),
                    proof_map.get(&height).unwrap().clone(),
                )
            })
            .collect()
    }

    pub fn get_latest_height(&self, from: &Address) -> Height {
        *self.latest_height_map.read().get(from).unwrap()
    }

    fn ensure_right_height(&self, address: &Address, height: Height) {
        let latest_height = *self.latest_height_map.read().get(address).unwrap();
        if height <= latest_height {
            println!(
                "{} save lower block of height {}, while it's latest_height is {}",
                address.tiny_hex(),
                height,
                latest_height
            );
            return;
        }
        if height > latest_height + 1 {
            panic!(
                "{} save higher block of height {}, while it's latest_height is {}",
                address.tiny_hex(),
                height,
                latest_height
            );
        }
    }
}
