use std::collections::HashMap;

use overlord::crypto::KeyPairs;
use overlord::types::{SelectMode, TimeConfig};
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
        if latest_height_map.get(&address).is_none() {
            (*latest_height_map).insert(address, 0);
        }
    }

    pub fn save_genesis_block(&self, key_pairs: &KeyPairs) {
        let genesis_block = Block::new(&key_pairs, SelectMode::InTurn, 5, TimeConfig::default());
        self.block_map.write().insert(0, genesis_block);
        self.proof_map.write().insert(0, Proof::default());
    }

    pub fn save_block_with_proof(&self, to: Address, height: Height, block: Block, proof: Proof) {
        self.ensure_right_height(&to, height);

        let mut block_map = self.block_map.write();
        if let Some(block_exists) = block_map.get(&height) {
            assert_eq!(
                block_exists,
                &block,
                "[TEST]\n\t<{}> -> storage\n\t<collapsed> save.block != exist.block, {} != {}\n",
                to.tiny_hex(),
                block,
                block_exists
            );
        } else {
            let mut proof_map = self.proof_map.write();
            block_map.insert(height, block);
            proof_map.insert(height, proof);
        }
        let latest_height = { *self.latest_height_map.read().get(&to).unwrap() };
        if height > latest_height {
            let mut latest_height_map = self.latest_height_map.write();
            latest_height_map.insert(to.clone(), height);
        }

        let list_vec: Vec<(String, Height)> = self
            .latest_height_map
            .read()
            .clone()
            .iter()
            .map(|(address, height)| (address.tiny_hex(), *height))
            .collect();
        println!(
            "[TEST]\n\t<{}> -> storage\n\t<update> storage: {:?}\n",
            to.tiny_hex(),
            list_vec,
        );
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
                "[TEST]\n\t<{}> -> storage\n\t<warn> save_block.height <= exist.height, {} <= {}\n",
                address.tiny_hex(),
                height,
                latest_height
            );
            return;
        }
        if height > latest_height + 1 {
            panic!(
                "[TEST]\n\t<{}> -> storage\n\t<collapsed> save_block.height > exist.height + 1, {} > {} + 1\n",
                address.tiny_hex(),
                height,
                latest_height
            );
        }
    }
}
