use bytes::Bytes;
use overlord::{Hash, Height, Proof};
use parking_lot::RwLock;
use rand::random;

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

    #[allow(dead_code)]
    pub fn send_tx(&self, new_tx: Transaction) {
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
        let tx = self.tx.read().clone();
        Block {
            pre_hash,
            height,
            exec_height,
            pre_proof,
            state_root,
            receipt_roots,
            nonce: Bytes::from(gen_random_bytes(10)),
            tx,
        }
    }
}

fn gen_random_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|_| random::<u8>()).collect::<Vec<_>>()
}
