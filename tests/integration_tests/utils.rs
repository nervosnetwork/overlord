use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytes::{Bytes, BytesMut};
use hasher::{Hasher, HasherKeccak};
use hummer::coding::hex_encode;
use lazy_static::lazy_static;
use rand::{random, seq::SliceRandom, thread_rng};

use overlord::types::Node;
use overlord::DurationConfig;

lazy_static! {
    static ref HASHER_INST: HasherKeccak = HasherKeccak::new();
}

pub fn gen_random_bytes() -> Bytes {
    let vec: Vec<u8> = (0..10).map(|_| random::<u8>()).collect();
    Bytes::from(vec)
}

pub fn hash(bytes: &Bytes) -> Bytes {
    let mut out = [0u8; 32];
    out.copy_from_slice(&HASHER_INST.digest(bytes));
    BytesMut::from(&out[..]).freeze()
}

pub fn timer_config() -> Option<DurationConfig> {
    Some(DurationConfig::new(20, 10, 5, 10))
}

pub fn create_alive_nodes(nodes: Vec<Node>) -> Vec<Node> {
    let node_num = nodes.len();
    let thresh_num = node_num * 2 / 3 + 1;
    let rand_num = 0;
    //    let rand_num = random::<usize>() % (node_num - thresh_num + 1);
    let mut alive_nodes = nodes;
    alive_nodes.shuffle(&mut thread_rng());
    while alive_nodes.len() > thresh_num + rand_num {
        alive_nodes.pop();
    }
    alive_nodes
}

pub fn get_max_alive_height(
    height_record: &Arc<Mutex<HashMap<Bytes, u64>>>,
    alives: &[Node],
) -> u64 {
    let height_record = height_record.lock().unwrap();
    if let Some(max_height) = height_record
        .clone()
        .into_iter()
        .filter(|(address, _)| alives.iter().any(|node| node.address == address))
        .collect::<HashMap<Bytes, u64>>()
        .values()
        .max()
    {
        *max_height
    } else {
        0
    }
}

pub fn to_hex_strings(nodes: &[Node]) -> Vec<String> {
    nodes.iter().map(|node| hex_encode(&node.address)).collect()
}

pub fn to_hex(address: &Bytes) -> String {
    hex_encode(address)
}

pub fn get_index_array(nodes: &[Node], alives: &[Node]) -> Vec<usize> {
    nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| alives.contains(node))
        .map(|(i, _)| i)
        .collect()
}

pub fn get_index(nodes: &[Node], address: &Bytes) -> usize {
    let mut index = std::usize::MAX;
    nodes.iter().enumerate().for_each(|(i, node)| {
        if node.address == address {
            index = i;
        }
    });
    index
}
