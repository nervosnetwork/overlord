#![allow(dead_code)]

use std::sync::Arc;

use bytes::Bytes;
use overlord::crypto::{KeyPair, KeyPairs};
use overlord::types::SelectMode;
use overlord::{
    gen_key_pairs, Address, CommonHex, DurationConfig, Node, OverlordConfig, OverlordServer,
};

use crate::common::adapter::OverlordAdapter;
use crate::common::block::Transaction;
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
    pub fn new(node_number: usize) -> Self {
        let network = Arc::new(Network::default());
        let storage = Arc::new(Storage::default());

        let key_pairs = gen_key_pairs(node_number, vec![], None);
        let common_ref_hex = key_pairs.common_ref.clone();

        let addresses = key_pairs.get_address_list();
        let init_config = init_config(&key_pairs.key_pairs);
        let mem_pool = Arc::new(MemPool::new(init_config));

        run_nodes(key_pairs, &network, &mem_pool, &storage);

        Platform {
            network,
            mem_pool,
            storage,
            addresses,
            common_ref_hex,
        }
    }

    // pub fn add_nodes(&self, node_number: usize) -> Vec<Node> {
    //     let key_pairs = gen_key_pairs(node_number, vec![], Some(self.common_ref_hex.clone()));
    //     let address_list = key_pairs.get_address_list();
    //     run_nodes(key_pairs, &self.network, &self.mem_pool, &self.storage);
    //     to_auth_list(address_list)
    // }

    pub fn set_auth_list(&self, address_list: Vec<Node>) {
        let old_tx = self.mem_pool.get_tx();

        let tx = Transaction {
            interval:        old_tx.interval,
            max_exec_behind: old_tx.max_exec_behind,
            mode:            old_tx.mode,
            timer_config:    old_tx.timer_config,
            auth_list:       address_list,
        };
        self.mem_pool.send_tx(tx)
    }
}

fn run_nodes(
    key_pairs: KeyPairs,
    network: &Arc<Network>,
    mem_pool: &Arc<MemPool>,
    storage: &Arc<Storage>,
) {
    // let common_ref_hex = key_pairs.common_ref.clone();
    let address_list = key_pairs.get_address_list();
    // let auth_list_hex: HashMap<AddressHex, BlsPubKeyHex> = key_pairs
    //     .key_pairs
    //     .iter()
    //     .map(|key_pair| (key_pair.address.clone(), key_pair.bls_public_key.clone()))
    //     .collect();

    address_list
        .into_iter()
        .zip(key_pairs.key_pairs)
        .for_each(|(address, key_pair)| {
            storage.register(address.clone());

            let adapter = Arc::new(OverlordAdapter::new(
                address.clone(),
                network,
                mem_pool,
                storage,
            ));

            let overlord_server = OverlordServer::new(address, &adapter, &key_pair.address);
            overlord_server.run()
        });
}

fn init_config(key_pairs: &[KeyPair]) -> OverlordConfig {
    OverlordConfig {
        interval:        3000,
        max_exec_behind: 5,
        mode:            SelectMode::InTurn,
        timer_config:    DurationConfig::default(),
        auth_list:       into_auth_list(key_pairs),
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
