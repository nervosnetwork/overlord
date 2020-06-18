use std::collections::HashMap;

use bit_vec::BitVec;
use derive_more::Display;
use prime_tools::get_primes_less_than_x;

use crate::error::ConsensusError;
use crate::types::{Address, Node};
use crate::utils::rand_proposer::get_random_proposer_index;
use crate::ConsensusResult;

/// Authority manage is an extensional data structure of authority list which means
/// `Vec<Node>`. It transforms the information in `Node` struct into a more suitable data structure
/// according to its usage scene. The vote weight need look up by address frequently, therefore,
/// address with vote weight saved in a `HashMap`.
#[display(fmt = "Authority List {:?}", address)]
#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub struct AuthorityManage {
    address:            Vec<Address>,
    propose_weights:    Vec<u64>,
    vote_weight_map:    HashMap<Address, u32>,
    propose_weight_sum: u64,
    vote_weight_sum:    u64,
}

impl AuthorityManage {
    /// Create a new height authority manage.
    pub fn new() -> Self {
        AuthorityManage {
            address:            Vec::new(),
            propose_weights:    Vec::new(),
            vote_weight_map:    HashMap::new(),
            propose_weight_sum: 0u64,
            vote_weight_sum:    0u64,
        }
    }

    /// Update the height authority manage by a new authority list.
    pub fn update(&mut self, authority_list: &mut Vec<Node>) {
        self.flush();
        authority_list.sort();

        for node in authority_list.iter_mut() {
            let propose_weight = u64::from(node.propose_weight);
            let vote_weight = node.vote_weight;

            self.address.push(node.address.clone());
            self.propose_weights.push(propose_weight);
            self.vote_weight_map
                .insert(node.address.clone(), vote_weight);
            self.propose_weight_sum += propose_weight;
            self.vote_weight_sum += u64::from(vote_weight);
        }
    }

    /// Get a vote weight of the node.
    pub fn get_vote_weight(&self, addr: &Address) -> ConsensusResult<&u32> {
        self.vote_weight_map
            .get(addr)
            .ok_or_else(|| ConsensusError::InvalidAddress)
    }

    /// Get the proposer address by a given seed.
    pub fn get_proposer(&self, height: u64, round: u64) -> ConsensusResult<Address> {
        let index = if cfg!(features = "random_leader") {
            get_random_proposer_index(
                height + round,
                &self.propose_weights,
                self.propose_weight_sum,
            )
        } else {
            rotation_leader_index(height, round, self.address.len())
        };

        if let Some(addr) = self.address.get(index) {
            return Ok(addr.to_owned());
        }
        Err(ConsensusError::Other(
            "The address list mismatch propose weight list".to_string(),
        ))
    }

    /// Calculate whether the sum of vote weights from bitmap is above 2/3.
    pub fn is_above_threshold(&self, bitmap: &[u8]) -> ConsensusResult<bool> {
        let bitmap = BitVec::from_bytes(&bitmap);
        let mut acc = 0u64;

        for node in bitmap.iter().zip(self.address.iter()) {
            if node.0 {
                if let Some(weight) = self.vote_weight_map.get(node.1) {
                    acc += u64::from(*weight);
                } else {
                    return Err(ConsensusError::Other(format!(
                        "Lose {:?} vote weight",
                        node.1.clone()
                    )));
                }
            }
        }

        Ok(acc * 3 > self.vote_weight_sum * 2)
    }

    pub fn get_voters(&self, bitmap: &[u8]) -> ConsensusResult<Vec<Address>> {
        let bitmap = BitVec::from_bytes(bitmap);
        let voters = bitmap
            .iter()
            .zip(self.address.iter())
            .filter(|node| node.0)
            .map(|node| node.1.clone())
            .collect::<Vec<_>>();
        Ok(voters)
    }

    /// If the given address is in the current authority list.
    pub fn contains(&self, address: &Address) -> bool {
        self.address.contains(address)
    }

    /// Get the sum of the vote weights in the current height.
    pub fn get_vote_weight_sum(&self) -> u64 {
        self.vote_weight_sum
    }

    /// Clear the HeightAuthorityManage, removing all values.
    pub fn flush(&mut self) {
        self.address.clear();
        self.propose_weights.clear();
        self.vote_weight_map.clear();
        self.propose_weight_sum = 0;
        self.vote_weight_sum = 0;
    }

    /// Get the length of the current authority list.
    pub fn len(&self) -> usize {
        self.address.len()
    }

    pub fn get_addres_ref(&self) -> &Vec<Address> {
        &self.address
    }
}

/// Give the validators list and bitmap, returns the activated validators, the authority list MUST
/// be sorted
pub fn extract_voters(
    authority_list: &mut Vec<Node>,
    address_bitmap: &bytes::Bytes,
) -> ConsensusResult<Vec<Address>> {
    authority_list.sort();
    let bitmap = BitVec::from_bytes(&address_bitmap);
    let voters: Vec<Address> = bitmap
        .iter()
        .zip(authority_list.iter())
        .filter(|pair| pair.0) //the bitmap must hit
        .map(|pair| pair.1.address.clone()) //get the corresponding address
        .collect::<Vec<_>>();
    Ok(voters)
}

/// Get the leader address of the height and the round, the authority list MUST be sorted.
pub fn rotation_leader(height: u64, round: u64, mut authority_list: Vec<Node>) -> Address {
    authority_list.sort();
    let mut weight_sum = 0;
    let mut propose_weights = Vec::new();
    for node in authority_list.iter() {
        weight_sum += node.propose_weight;
        propose_weights.push(node.propose_weight as u64);
    }

    let index = if cfg!(features = "random_leader") {
        get_random_proposer_index(height + round, &propose_weights, weight_sum as u64)
    } else {
        rotation_leader_index(height, round, authority_list.len())
    };

    authority_list[index].address.clone()
}

fn rotation_leader_index(height: u64, round: u64, authority_len: usize) -> usize {
    let len = authority_len as u32;
    let prime_num = *get_primes_less_than_x(len).last().unwrap_or(&1) as u64;
    let res = (height * prime_num + round) % (len as u64);
    res as usize
}

#[cfg(test)]
mod test {
    extern crate test;

    use bit_vec::BitVec;
    use bytes::Bytes;
    use rand::random;
    use test::Bencher;

    use crate::error::ConsensusError;
    use crate::extract_voters;
    use crate::types::{Address, Node};
    use crate::utils::auth_manage::AuthorityManage;

    fn gen_address() -> Address {
        Address::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>())
    }

    fn gen_auth_list(len: usize) -> Vec<Node> {
        if len == 0 {
            return vec![];
        }

        let mut authority_list = Vec::new();
        for _ in 0..len {
            authority_list.push(gen_node(gen_address(), random::<u32>(), random::<u32>()));
        }
        authority_list
    }

    fn gen_node(addr: Address, propose_weight: u32, vote_weight: u32) -> Node {
        let mut node = Node::new(addr);
        node.set_propose_weight(propose_weight);
        node.set_vote_weight(vote_weight);
        node
    }

    fn gen_bitmap(len: usize, nbits: Vec<usize>) -> BitVec {
        let mut bv = BitVec::from_elem(len, false);
        for n in nbits.into_iter() {
            bv.set(n, true);
        }
        bv
    }

    #[test]
    fn test_vote_weight() {
        let mut authority_list = gen_auth_list(0);
        let mut authority_manage = AuthorityManage::new();
        authority_manage.update(&mut authority_list);

        for node in authority_list.iter() {
            assert_eq!(
                authority_manage.get_vote_weight(&node.address),
                Err(ConsensusError::InvalidAddress)
            );
        }

        let mut auth_len = random::<u8>();
        while auth_len == 0 {
            auth_len = random::<u8>();
        }
        authority_manage.update(&mut gen_auth_list(auth_len as usize));

        for node in authority_list.iter() {
            assert_eq!(
                *authority_manage.get_vote_weight(&node.address).unwrap(),
                node.propose_weight
            );
        }
    }

    #[test]
    fn test_update() {
        let mut authority_list = gen_auth_list(random::<u8>() as usize);
        let mut auth_manage = AuthorityManage::new();
        auth_manage.update(&mut authority_list);
        assert_eq!(
            auth_manage.address,
            authority_list
                .into_iter()
                .map(|node| node.address)
                .collect::<Vec<Address>>()
        );
    }

    #[test]
    fn test_vote_threshold() {
        let mut authority_list = vec![
            gen_node(gen_address(), 1u32, 1u32),
            gen_node(gen_address(), 1u32, 1u32),
            gen_node(gen_address(), 1u32, 1u32),
            gen_node(gen_address(), 1u32, 1u32),
        ];
        authority_list.sort();
        let mut authority = AuthorityManage::new();
        authority.update(&mut authority_list);

        for i in 0..4 {
            let bit_map = gen_bitmap(4, vec![i]);
            let res = authority.is_above_threshold(Bytes::from(bit_map.to_bytes()).as_ref());
            assert_eq!(res.unwrap(), false);
        }

        let tmp = vec![vec![1, 2], vec![1, 3], vec![2, 3], vec![0, 1], vec![0, 2]];
        for i in tmp.into_iter() {
            let bit_map = gen_bitmap(4, i);
            let res = authority.is_above_threshold(Bytes::from(bit_map.to_bytes()).as_ref());
            assert_eq!(res.unwrap(), false);
        }

        let tmp = vec![vec![0, 1, 2], vec![0, 1, 3], vec![1, 2, 3]];
        for i in tmp.into_iter() {
            let bit_map = gen_bitmap(4, i);
            let res = authority.is_above_threshold(Bytes::from(bit_map.to_bytes()).as_ref());
            assert_eq!(res.unwrap(), true);
        }

        let bit_map = gen_bitmap(4, vec![0, 1, 2, 3]);
        let res = authority.is_above_threshold(Bytes::from(bit_map.to_bytes()).as_ref());
        assert_eq!(res.unwrap(), true);
    }

    #[test]
    fn test_bitmap() {
        let len = random::<u8>() as usize;
        let bitmap = (0..len).map(|_| random::<bool>()).collect::<Vec<_>>();
        let mut bv = BitVec::from_elem(len, false);
        for (index, is_vote) in bitmap.iter().enumerate() {
            if *is_vote {
                bv.set(index, true);
            }
        }

        let tmp = Bytes::from(bv.to_bytes());
        let output = BitVec::from_bytes(tmp.as_ref());

        for item in output.iter().zip(bitmap.iter()) {
            assert_eq!(item.0, *item.1);
        }
    }

    #[test]
    fn test_get_voters() {
        let auth_list = (0..4).map(|_| gen_address()).collect::<Vec<_>>();
        let bitmap = BitVec::from_bytes(&[0b1010_0000]);
        let voters = bitmap
            .iter()
            .zip(auth_list.iter())
            .filter(|node| node.0)
            .map(|node| node.1.clone())
            .collect::<Vec<_>>();

        assert_eq!(voters.len(), 2);
        assert_eq!(voters[0], auth_list[0]);
        assert_eq!(voters[1], auth_list[2]);
    }

    #[test]
    fn test_poll_leader() {
        let mut authority_list = vec![
            gen_node(gen_address(), 1u32, 1u32),
            gen_node(gen_address(), 1u32, 1u32),
            gen_node(gen_address(), 1u32, 1u32),
            gen_node(gen_address(), 1u32, 1u32),
        ];
        authority_list.sort();
        let mut authority = AuthorityManage::new();
        authority.update(&mut authority_list);

        assert_eq!(
            authority.get_proposer(1, 0).unwrap(),
            authority_list[3].address
        );
        assert_eq!(
            authority.get_proposer(1, 1).unwrap(),
            authority_list[0].address
        );
        assert_eq!(
            authority.get_proposer(2, 0).unwrap(),
            authority_list[2].address
        );
        assert_eq!(
            authority.get_proposer(2, 2).unwrap(),
            authority_list[0].address
        );
        assert_eq!(
            authority.get_proposer(3, 0).unwrap(),
            authority_list[1].address
        );
        assert_eq!(
            authority.get_proposer(3, 1).unwrap(),
            authority_list[2].address
        );
    }

    #[test]
    fn test_extract_voters() {
        let mut auth_list = gen_auth_list(10);
        let mut origin_auth_list = auth_list.clone();
        origin_auth_list.sort();
        let bit_map = gen_bitmap(10, vec![0, 1, 2]);
        let res = extract_voters(&mut auth_list, &Bytes::from(bit_map.to_bytes())).unwrap();

        assert_eq!(res.len(), 3);

        for (index, address) in res.iter().enumerate() {
            assert_eq!(
                origin_auth_list.get(index).unwrap().address,
                address.clone()
            );
        }
    }

    #[bench]
    fn bench_update(b: &mut Bencher) {
        let mut auth_list = gen_auth_list(10);
        let mut authority = AuthorityManage::new();
        b.iter(|| authority.update(&mut auth_list));
    }

    #[bench]
    fn bench_cal_vote_weight(b: &mut Bencher) {
        let mut auth_list = gen_auth_list(10);
        let mut authority = AuthorityManage::new();
        authority.update(&mut auth_list);
        let bitmap = BitVec::from_elem(10, true);
        let vote_bitmap = Bytes::from(bitmap.to_bytes());
        b.iter(|| authority.is_above_threshold(&vote_bitmap));
    }
}
