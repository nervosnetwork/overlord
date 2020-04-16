#![allow(dead_code)]

use std::sync::Arc;

use bytes::Bytes;
use overlord::crypto::{KeyPair, KeyPairs};
use overlord::types::SelectMode;
use overlord::{
    gen_key_pairs, Address, AuthConfig, CommonHex, Node, OverlordConfig, OverlordServer, Proof,
    TimeConfig,
};

use crate::common::adapter::OverlordAdapter;
use crate::common::block::Block;
use crate::common::mem_pool::MemPool;
use crate::common::network::Network;
use crate::common::storage::Storage;

pub struct Platform {
    network:  Arc<Network>,
    mem_pool: Arc<MemPool>,
    storage:  Arc<Storage>,

    addresses:      Vec<Address>,
    common_ref_hex: CommonHex,
}

impl Platform {
    pub async fn new(node_number: usize) -> Self {
        let network = Arc::new(Network::default());
        let storage = Arc::new(Storage::default());

        let key_pairs = gen_key_pairs(node_number, vec![], None);
        let common_ref_hex = key_pairs.common_ref.clone();

        let addresses = key_pairs.get_address_list();
        let init_config = init_config(&key_pairs);
        let mem_pool = Arc::new(MemPool::new(init_config));

        run_nodes(key_pairs, &network, &mem_pool, &storage).await;

        Platform {
            network,
            mem_pool,
            storage,
            addresses,
            common_ref_hex,
        }
    }

    pub fn set_consensus_config(&self, overlord_config: OverlordConfig) {
        self.mem_pool.send_tx(overlord_config)
    }
}

async fn run_nodes(
    key_pairs: KeyPairs,
    network: &Arc<Network>,
    mem_pool: &Arc<MemPool>,
    storage: &Arc<Storage>,
) {
    let common_ref = key_pairs.common_ref.clone();
    let genesis_block = Block::new(&key_pairs, SelectMode::InTurn, 5, TimeConfig::default());
    for key_pair in key_pairs.key_pairs {
        let address = hex_to_address(&key_pair.address);
        storage.register(address.clone());
        storage.save_block_with_proof(address.clone(), 0, genesis_block.clone(), Proof::default());
        let adapter = Arc::new(OverlordAdapter::new(
            address.clone(),
            network,
            mem_pool,
            storage,
        ));
        OverlordServer::run(
            common_ref.clone(),
            key_pair.private_key.clone(),
            address,
            &adapter,
            &("wal/tests/".to_owned() + &key_pair.address),
        )
        .await;
    }
}

fn init_config(key_pairs: &KeyPairs) -> OverlordConfig {
    let auth_config = AuthConfig {
        common_ref: key_pairs.common_ref.clone(),
        mode:       SelectMode::InTurn,
        auth_list:  into_auth_list(&key_pairs.key_pairs),
    };
    OverlordConfig {
        max_exec_behind: 5,
        time_config: TimeConfig::default(),
        auth_config,
    }
}

fn into_auth_list(key_pairs: &[KeyPair]) -> Vec<Node> {
    key_pairs
        .iter()
        .map(|key_pair| {
            Node::new(
                hex_to_address(&key_pair.address),
                key_pair.bls_public_key.clone(),
                1,
                1,
            )
        })
        .collect()
}

fn hex_to_address(hex_str: &str) -> Address {
    Bytes::from(hex_decode(hex_str))
}

fn hex_decode(hex_str: &str) -> Vec<u8> {
    hex::decode(ensure_trim0x(hex_str)).unwrap()
}

fn ensure_trim0x(str: &str) -> &str {
    if str.starts_with("0x") || str.starts_with("0X") {
        &str[2..]
    } else {
        str
    }
}
