use overlord::{Hash, Height, Proof};
use parking_lot::RwLock;

use crate::common::block::{Block, Transaction};

pub struct MemPool {
    tx: RwLock<Transaction>,
}

impl MemPool {
    pub fn new(tx: Transaction) -> MemPool {
        MemPool {
            tx: RwLock::new(tx),
        }
    }

    pub fn send_transaction(&self, new_tx: Transaction) {
        let mut tx = self.tx.write();
        *tx = new_tx;
    }

    pub fn package(
        &self,
        height: Height,
        exec_height: Height,
        pre_hash: Hash,
        pre_proof: Proof,
        state_root: Hash,
        receipt_roots: Vec<Hash>,
    ) -> Block {
        if height == 0 {
            return Block::genesis_block();
        }

        let tx = self.tx.read().clone();
        Block {
            pre_hash,
            height,
            exec_height,
            pre_proof,
            state_root,
            receipt_roots,
            tx,
        }
    }
}
