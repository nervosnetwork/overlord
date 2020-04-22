use bytes::Bytes;
use overlord::{Hash, Height, Node, Proof, TinyHex};
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

    pub fn auth_new_node(&self, node: Node) {
        let mut tx = self.tx.write();
        if tx
            .auth_config
            .auth_list
            .iter()
            .find(|n| n == &&node)
            .is_none()
        {
            tx.auth_config.auth_list.push(node);
        }
    }

    pub fn remove_node_auth(&self, node: Node) {
        let mut tx = self.tx.write();
        let index = tx
            .auth_config
            .auth_list
            .iter()
            .position(|n| *n == node)
            .unwrap_or_else(|| {
                panic!(
                    "cannot find index of {} in auth_config",
                    node.address.tiny_hex()
                )
            });
        tx.auth_config.auth_list.remove(index);
    }

    pub fn update_interval(&self, interval: u64) {
        let mut tx = self.tx.write();
        tx.time_config.interval = interval;
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
