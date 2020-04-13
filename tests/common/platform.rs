use std::collections::HashMap;
use std::sync::Arc;

use crate::common::adapter::OverlordAdapter;
use crate::common::block::Transaction;
use crate::common::mem_pool::MemPool;
use crate::common::network::Network;
use crate::common::storage::Storage;
use overlord::crypto::{CommonRefHex, KeyPairs};
use overlord::{
    gen_key_pairs, Address, AddressHex, BlsPubKeyHex, ConsensusConfig, DefaultCrypto,
    DurationConfig, Node, OverlordServer,
};

#[allow(dead_code)]
pub struct Platform {
    network:  Arc<Network>,
    mem_pool: Arc<MemPool>,
    storage:  Arc<Storage>,

    addresses:      Vec<Address>,
    common_ref_hex: CommonRefHex,
}

impl Platform {
    pub fn new(node_number: usize) -> Self {
        let network = Arc::new(Network::default());
        let storage = Arc::new(Storage::default());

        let key_pairs = gen_key_pairs(node_number, vec![], None);
        let addresses = key_pairs.get_address_list();
        let init_config = init_config(addresses.clone());
        let mem_pool = Arc::new(MemPool::new(init_config));

        run_nodes(key_pairs, &network, &mem_pool, &storage);

        let common_ref_hex = key_pairs.common_ref.clone();

        Platform {
            network,
            mem_pool,
            storage,
            addresses,
            common_ref_hex,
        }
    }

    #[allow(dead_code)]
    pub fn add_nodes(&self, node_number: usize) -> Vec<Node> {
        let key_pairs = gen_key_pairs(node_number, vec![], Some(self.common_ref_hex.clone()));
        let address_list = key_pairs.get_address_list();
        run_nodes(key_pairs, &self.network, &self.mem_pool, &self.storage);
        to_auth_list(address_list)
    }

    #[allow(dead_code)]
    pub fn set_auth_list(&self, address_list: Vec<Node>) {
        let old_tx = self.mem_pool.get_tx();

        let tx = Transaction {
            interval:        old_tx.interval,
            max_exec_behind: old_tx.max_exec_behind,
            timer_config:    old_tx.timer_config,
            authority_list:  address_list,
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
    let common_ref_hex = key_pairs.common_ref.clone();
    let address_list = key_pairs.get_address_list();
    let auth_list_hex: HashMap<AddressHex, BlsPubKeyHex> = key_pairs
        .key_pairs
        .iter()
        .map(|key_pair| (key_pair.address.clone(), key_pair.bls_public_key.clone()))
        .collect();

    address_list
        .into_iter()
        .zip(key_pairs.key_pairs)
        .for_each(|(address, key_pair)| {
            storage.register(address.clone());

            let adapter = OverlordAdapter::new(address.clone(), network, mem_pool, storage);
            let crypto = DefaultCrypto::new(
                key_pair.private_key,
                auth_list_hex.clone(),
                common_ref_hex.clone(),
            );

            let overlord_server = OverlordServer::new(address, adapter, crypto, &key_pair.address);
            overlord_server.run()
        });
}

fn init_config(address_list: Vec<Address>) -> ConsensusConfig {
    ConsensusConfig {
        interval:        3000,
        max_exec_behind: 5,
        timer_config:    DurationConfig::default(),
        authority_list:  to_auth_list(address_list),
    }
}

fn to_auth_list(address_list: Vec<Address>) -> Vec<Node> {
    address_list.into_iter().map(Node::new).collect()
}
