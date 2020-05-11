use std::sync::Arc;

use bytes::Bytes;
use overlord::crypto::{KeyPair, KeyPairs};
use overlord::types::SelectMode;
use overlord::{
    gen_key_pairs, Address, AuthConfig, CryptoConfig, Height, Node, OverlordConfig, OverlordMsg,
    OverlordServer, TimeConfig, TinyHex,
};

use crate::common::adapter::OverlordAdapter;
use crate::common::mem_pool::MemPool;
use crate::common::network::Network;
use crate::common::storage::Storage;
use creep::Context;

pub struct Platform {
    network:  Arc<Network>,
    mem_pool: Arc<MemPool>,
    storage:  Arc<Storage>,

    key_pairs: KeyPairs,
}

impl Platform {
    pub fn new(node_number: usize) -> Self {
        let network = Arc::new(Network::default());
        let storage = Arc::new(Storage::default());

        let key_pairs = gen_key_pairs(node_number, vec![], None);

        let init_config = init_config(&key_pairs);
        let mem_pool = Arc::new(MemPool::new(init_config));

        storage.save_genesis_block(&key_pairs);

        run(key_pairs.clone(), &network, &mem_pool, &storage);
        Platform {
            network,
            mem_pool,
            storage,
            key_pairs,
        }
    }

    pub fn add_new_nodes(&mut self, node_number: usize) -> Vec<Address> {
        let common_ref = self.key_pairs.common_ref.clone();
        let key_pairs = gen_key_pairs(node_number, vec![], Some(common_ref));
        self.key_pairs
            .key_pairs
            .append(&mut key_pairs.key_pairs.clone());
        run(
            key_pairs.clone(),
            &self.network,
            &self.mem_pool,
            &self.storage,
        );
        into_address_list(&key_pairs.key_pairs)
    }

    pub fn restart_node(&self, node: &Address) {
        let common_ref = self.key_pairs.common_ref.clone();
        let key_pair = self
            .key_pairs
            .key_pairs
            .iter()
            .find(|key_pair| hex_to_address(&key_pair.address) == node)
            .expect("restart node failed");
        let key_pairs = KeyPairs {
            common_ref,
            key_pairs: vec![key_pair.clone()],
        };
        run(key_pairs, &self.network, &self.mem_pool, &self.storage);
    }

    pub fn stop_node(&self, address: &Address) {
        let _ = self.network.transmit(address, OverlordMsg::Stop);
    }

    pub fn auth_new_node(&self, address: &Address) {
        let node = self.get_node(address);
        self.mem_pool.auth_new_node(node);
    }

    pub fn remove_node_auth(&self, address: &Address) {
        let node = self.get_node(address);
        self.mem_pool.remove_node_auth(node);
    }

    pub fn update_interval(&self, interval: u64) {
        self.mem_pool.update_interval(interval);
    }

    pub fn get_latest_height(&self, address: &Address) -> Height {
        self.storage.get_latest_height(address)
    }

    pub fn get_address_list(&self) -> Vec<Address> {
        into_address_list(&self.key_pairs.key_pairs)
    }

    fn get_node(&self, address: &Address) -> Node {
        let key_pair = self
            .key_pairs
            .key_pairs
            .iter()
            .find(|key_pair| hex_to_address(&key_pair.address) == address)
            .expect("get node failed");
        Node::new(
            hex_to_address(&key_pair.address),
            key_pair.bls_public_key.clone(),
            1,
            1,
        )
    }
}

fn run(
    key_pairs: KeyPairs,
    network: &Arc<Network>,
    mem_pool: &Arc<MemPool>,
    storage: &Arc<Storage>,
) {
    let common_ref = key_pairs.common_ref.clone();
    for key_pair in key_pairs.key_pairs {
        let address = hex_to_address(&key_pair.address);
        storage.register(address.clone());
        let adapter = Arc::new(OverlordAdapter::new(
            address.clone(),
            network,
            mem_pool,
            storage,
        ));

        let tiny_address = address.tiny_hex();
        let crypto_config = CryptoConfig::new(
            common_ref.clone(),
            key_pair.private_key.clone(),
            key_pair.public_key.clone(),
            key_pair.bls_public_key.clone(),
            address,
        );
        tokio::spawn(async move {
            OverlordServer::run(
                Context::default(),
                crypto_config,
                &adapter,
                &("wal/tests/".to_owned() + &tiny_address),
            )
            .await;
        });
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

fn into_address_list(key_pairs: &[KeyPair]) -> Vec<Address> {
    key_pairs
        .iter()
        .map(|keypair| hex_to_address(&keypair.address))
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
