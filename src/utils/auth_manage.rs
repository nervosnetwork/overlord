use std::collections::HashMap;

use bit_vec::BitVec;
use derive_more::Display;
use prime_tools::get_primes_less_than_x;

use crate::error::ConsensusError;
use crate::types::{Address, Node};
use crate::utils::rand_proposer::get_random_proposer_index;
use crate::ConsensusResult;

/// Authority manage consits of the current height authority manage and the last height authority
/// manage The last height authority manage which is optional is used to check the correctness of
/// last height's message. The last height's authority manage should be `Some` unless current height
/// is `0` or `1`.
#[display(fmt = "Authority List {:?}", current)]
#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub struct AuthorityManage {
    current: HeightAuthorityManage,
    last:    Option<HeightAuthorityManage>,
}

impl AuthorityManage {
    /// Create a new authority management.
    pub fn new() -> Self {
        AuthorityManage {
            current: HeightAuthorityManage::new(),
            last:    None,
        }
    }

    /// Update a new height of authority list. If the argument `reserve_old` is `true`, the old
    /// authority will be reserved, otherwise, it will be cleared.
    pub fn update(&mut self, authority_list: &mut Vec<Node>, reserve_old: bool) {
        self.last = if reserve_old {
            Some(self.current.clone())
        } else {
            None
        };
        self.current.update(authority_list);
    }

    /// Set the last height's authority list when the node leap to a higher height. In this
    /// situation, an authority list of `current_height - 1` is required to guarantee the
    /// consensus liveness.
    pub fn set_last_list(&mut self, authority_list: &mut Vec<Node>) {
        let mut auth_manage = HeightAuthorityManage::new();
        auth_manage.update(authority_list);
        self.last = Some(auth_manage);
    }

    /// Get a vote weight that correspond to the given address. Return `Err` when the given address
    /// is not in the authority list.
    pub fn get_vote_weight(&self, addr: &Address) -> ConsensusResult<&u32> {
        self.current.get_vote_weight(addr)
    }

    /// Get a proposer address of the height by a given seed. Return `Err` when `is_current` is
    /// `false`, and the when last height's authority management is `None`.
    pub fn get_proposer(
        &self,
        height: u64,
        round: u64,
        is_current: bool,
    ) -> ConsensusResult<Address> {
        if is_current {
            self.current.get_proposer(height, round)
        } else if let Some(auth_list) = self.last.clone() {
            auth_list.get_proposer(height, round)
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last height".to_string(),
            ))
        }
    }

    /// Calculate whether the sum of vote weights from bitmap is above 2/3. Return `Err` when
    /// `is_current` is `false`, and the when last height's authority management is `None`.
    pub fn is_above_threshold(&self, bitmap: &[u8], is_current: bool) -> ConsensusResult<bool> {
        if is_current {
            self.current.is_above_threshold(bitmap)
        } else if let Some(auth_list) = self.last.clone() {
            auth_list.is_above_threshold(bitmap)
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last height".to_string(),
            ))
        }
    }

    /// According the given bitmap to get the voters address to verify aggregated vote.
    pub fn get_voters(&self, bitmap: &[u8], is_current: bool) -> ConsensusResult<Vec<Address>> {
        if is_current {
            self.current.get_voters(bitmap)
        } else if let Some(auth_list) = self.last.clone() {
            auth_list.get_voters(bitmap)
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last height".to_string(),
            ))
        }
    }

    /// Check whether the authority management contains the given address. Return `Err` when
    /// `is_current` is `false`, and the last height's authority management is `None`.
    pub fn contains(&self, address: &Address, is_current: bool) -> ConsensusResult<bool> {
        if is_current {
            Ok(self.current.contains(address))
        } else if let Some(auth_list) = self.last.clone() {
            Ok(auth_list.contains(address))
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last height".to_string(),
            ))
        }
    }

    /// Get the sum of the vote weights. Return `Err` when `is_current` is `false`, and the last
    /// height's authority management is `None`.
    pub fn get_vote_weight_sum(&self, is_current: bool) -> ConsensusResult<u64> {
        if is_current {
            Ok(self.current.get_vote_weight_sum())
        } else if let Some(auth_list) = self.last.clone() {
            Ok(auth_list.get_vote_weight_sum())
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last height".to_string(),
            ))
        }
    }

    /// Get the length of the current authority list.
    pub fn current_len(&self) -> usize {
        self.current.address.len()
    }

    pub fn get_addres_ref(&self) -> &Vec<Address> {
        &self.current.address
    }
}

/// Height authority manage is an extensional data structure of authority list which means
/// `Vec<Node>`. It transforms the information in `Node` struct into a more suitable data structure
/// according to its usage scene. The vote weight need look up by address frequently, therefore,
/// address with vote weight saved in a `HashMap`.
#[display(fmt = "{:?}", address)]
#[derive(Clone, Debug, Display, PartialEq, Eq)]
struct HeightAuthorityManage {
    address:            Vec<Address>,
    propose_weights:    Vec<u64>,
    vote_weight_map:    HashMap<Address, u32>,
    propose_weight_sum: u64,
    vote_weight_sum:    u64,
}

impl HeightAuthorityManage {
    /// Create a new height authority manage.
    fn new() -> Self {
        HeightAuthorityManage {
            address:            Vec::new(),
            propose_weights:    Vec::new(),
            vote_weight_map:    HashMap::new(),
            propose_weight_sum: 0u64,
            vote_weight_sum:    0u64,
        }
    }

    /// Update the height authority manage by a new authority list.
    fn update(&mut self, authority_list: &mut Vec<Node>) {
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
    fn get_vote_weight(&self, addr: &Address) -> ConsensusResult<&u32> {
        self.vote_weight_map
            .get(addr)
            .ok_or_else(|| ConsensusError::InvalidAddress)
    }

    /// Get the proposer address by a given seed.
    fn get_proposer(&self, height: u64, round: u64) -> ConsensusResult<Address> {
        let index = if cfg!(features = "random_leader") {
            get_random_proposer_index(
                height + round,
                &self.propose_weights,
                self.propose_weight_sum,
            )
        } else {
            let len = self.address.len();
            let prime_num = *get_primes_less_than_x(len as u32).last().unwrap_or(&1) as u64;
            let res = (height * prime_num + round) % (len as u64);
            res as usize
        };

        if let Some(addr) = self.address.get(index) {
            return Ok(addr.to_owned());
        }
        Err(ConsensusError::Other(
            "The address list mismatch propose weight list".to_string(),
        ))
    }

    /// Calculate whether the sum of vote weights from bitmap is above 2/3.
    fn is_above_threshold(&self, bitmap: &[u8]) -> ConsensusResult<bool> {
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

    fn get_voters(&self, bitmap: &[u8]) -> ConsensusResult<Vec<Address>> {
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
    fn contains(&self, address: &Address) -> bool {
        self.address.contains(address)
    }

    /// Get the sum of the vote weights in the current height.
    fn get_vote_weight_sum(&self) -> u64 {
        self.vote_weight_sum
    }

    /// Clear the HeightAuthorityManage, removing all values.
    fn flush(&mut self) {
        self.address.clear();
        self.propose_weights.clear();
        self.vote_weight_map.clear();
        self.propose_weight_sum = 0;
        self.vote_weight_sum = 0;
    }
}

//give the validators list and bitmap, returns the activated validators, the authority_list MUST be sorted
pub fn extract_voters(authority_list: &mut Vec<Node>, address_bitmap: &bytes::Bytes) ->ConsensusResult<Vec<Address>>{
    authority_list.sort();
    let bitmap = BitVec::from_bytes(&address_bitmap);
    let voters:Vec<Address> = bitmap
        .iter()
        .zip(authority_list.iter())
        .filter(|pair| pair.0) //the bitmap must hit
        .map(|pair| pair.1.address.clone()) //get the corresponding address
        .collect::<Vec<_>>();
    return Ok(voters);
}

#[cfg(test)]
mod test {
    extern crate test;

    use bit_vec::BitVec;
    use bytes::Bytes;
    use rand::random;
    use test::Bencher;

    use crate::error::ConsensusError;
    use crate::types::{Address, Node};
    use crate::utils::auth_manage::{AuthorityManage, HeightAuthorityManage};
    use crate::extract_voters;

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
        let mut authority_manage = HeightAuthorityManage::new();
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
        let mut authority_list_new = gen_auth_list(random::<u8>() as usize);
        let mut authority_list_newer = gen_auth_list(random::<u8>() as usize);
        let mut auth_manage = AuthorityManage::new();
        let mut e_auth_manage = HeightAuthorityManage::new();

        auth_manage.update(&mut authority_list, false);
        e_auth_manage.update(&mut authority_list);
        assert_eq!(auth_manage, AuthorityManage {
            current: e_auth_manage.clone(),
            last:    None,
        });

        let mut e_auth_manage_old = e_auth_manage.clone();
        auth_manage.update(&mut authority_list_new, true);
        e_auth_manage.update(&mut authority_list_new);
        assert_eq!(auth_manage, AuthorityManage {
            current: e_auth_manage.clone(),
            last:    Some(e_auth_manage_old),
        });

        e_auth_manage_old = e_auth_manage.clone();
        auth_manage.update(&mut authority_list_newer, true);
        e_auth_manage.update(&mut authority_list_newer);
        assert_eq!(auth_manage, AuthorityManage {
            current: e_auth_manage.clone(),
            last:    Some(e_auth_manage_old),
        });

        auth_manage.update(&mut authority_list_new, false);
        e_auth_manage.update(&mut authority_list_new);
        assert_eq!(auth_manage, AuthorityManage {
            current: e_auth_manage,
            last:    None,
        });
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
        authority.update(&mut authority_list, false);

        for i in 0..4 {
            let bit_map = gen_bitmap(4, vec![i]);
            let res = authority.is_above_threshold(Bytes::from(bit_map.to_bytes()).as_ref(), true);
            assert_eq!(res.unwrap(), false);
        }

        let tmp = vec![vec![1, 2], vec![1, 3], vec![2, 3], vec![0, 1], vec![0, 2]];
        for i in tmp.into_iter() {
            let bit_map = gen_bitmap(4, i);
            let res = authority.is_above_threshold(Bytes::from(bit_map.to_bytes()).as_ref(), true);
            assert_eq!(res.unwrap(), false);
        }

        let tmp = vec![vec![0, 1, 2], vec![0, 1, 3], vec![1, 2, 3]];
        for i in tmp.into_iter() {
            let bit_map = gen_bitmap(4, i);
            let res = authority.is_above_threshold(Bytes::from(bit_map.to_bytes()).as_ref(), true);
            assert_eq!(res.unwrap(), true);
        }

        let bit_map = gen_bitmap(4, vec![0, 1, 2, 3]);
        let res = authority.is_above_threshold(Bytes::from(bit_map.to_bytes()).as_ref(), true);
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
        authority.update(&mut authority_list, false);

        assert_eq!(
            authority.get_proposer(1, 0, true).unwrap(),
            authority_list[3].address
        );
        assert_eq!(
            authority.get_proposer(1, 1, true).unwrap(),
            authority_list[0].address
        );
        assert_eq!(
            authority.get_proposer(2, 0, true).unwrap(),
            authority_list[2].address
        );
        assert_eq!(
            authority.get_proposer(2, 2, true).unwrap(),
            authority_list[0].address
        );
        assert_eq!(
            authority.get_proposer(3, 0, true).unwrap(),
            authority_list[1].address
        );
        assert_eq!(
            authority.get_proposer(3, 1, true).unwrap(),
            authority_list[2].address
        );
    }

    #[test]
    fn test_extract_voters(){
        let mut auth_list = gen_auth_list(10);
        let mut origin_auth_list = auth_list.clone();
        origin_auth_list.sort();
        let bit_map = gen_bitmap(10,vec![0,1,2]);
        let res = extract_voters(&mut auth_list,&Bytes::from(bit_map.to_bytes())).unwrap();

        assert_eq!(res.len(),3);

        for (index, address) in res.iter().enumerate(){
            assert_eq!(origin_auth_list.get(index).unwrap().address,address.clone());
        }
    }

    #[bench]
    fn bench_update(b: &mut Bencher) {
        let mut auth_list = gen_auth_list(10);
        let mut authority = AuthorityManage::new();
        b.iter(|| authority.update(&mut auth_list, true));
    }

    #[bench]
    fn bench_cal_vote_weight(b: &mut Bencher) {
        let mut auth_list = gen_auth_list(10);
        let mut authority = AuthorityManage::new();
        authority.update(&mut auth_list, false);
        let bitmap = BitVec::from_elem(10, true);
        let vote_bitmap = Bytes::from(bitmap.to_bytes());
        b.iter(|| authority.is_above_threshold(&vote_bitmap, true));
    }
}
