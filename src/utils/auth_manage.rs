use std::collections::HashMap;

use bit_vec::BitVec;
use derive_more::Display;

use crate::error::ConsensusError;
use crate::types::{Address, Node};
use crate::utils::rand_proposer::get_proposer_index;
use crate::ConsensusResult;

/// Authority manage consits of the current epoch authority manage and the last epoch authority
/// manage The last epoch authority manage which is optional is used to check the correctness of
/// last epoch's message. The last epoch's authority manage should be `Some` unless current epoch is
/// `0` or `1`.
#[display(fmt = "Authority List {:?}", current)]
#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub struct AuthorityManage {
    current: EpochAuthorityManage,
    last:    Option<EpochAuthorityManage>,
}

impl AuthorityManage {
    /// Create a new authority management.
    pub fn new() -> Self {
        AuthorityManage {
            current: EpochAuthorityManage::new(),
            last:    None,
        }
    }

    /// Update a new epoch of authority list. If the argument `reserve_old` is `true`, the old
    /// authority will be reserved, otherwise, it will be cleared.
    pub fn update(&mut self, authority_list: &mut Vec<Node>, reserve_old: bool) {
        self.last = if reserve_old {
            Some(self.current.clone())
        } else {
            None
        };
        self.current.update(authority_list);
    }

    /// Set the last epoch's authority list when the node leap to a higher epoch. In this situation,
    /// an authority list of `current_epoch - 1` is required to guarantee the consensus
    /// liveness.
    pub fn set_last_list(&mut self, authority_list: &mut Vec<Node>) {
        let mut auth_manage = EpochAuthorityManage::new();
        auth_manage.update(authority_list);
        self.last = Some(auth_manage);
    }

    /// Get a vote weight that correspond to the given address. Return `Err` when the given address
    /// is not in the authority list.
    pub fn get_vote_weight(&self, addr: &Address) -> ConsensusResult<&u8> {
        self.current.get_vote_weight(addr)
    }

    /// Get a proposer address of the epoch by a given seed. Return `Err` when `is_current` is
    /// `false`, and the when last epoch ID's authority management is `None`.
    pub fn get_proposer(&self, seed: u64, is_current: bool) -> ConsensusResult<Address> {
        if is_current {
            self.current.get_proposer(seed)
        } else if let Some(auth_list) = self.last.clone() {
            auth_list.get_proposer(seed)
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last epoch".to_string(),
            ))
        }
    }

    /// Calculate whether the sum of vote weights from bitmap is above 2/3. Return `Err` when
    /// `is_current` is `false`, and the when last epoch ID's authority management is `None`.
    pub fn is_above_threshold(&self, bitmap: &[u8], is_current: bool) -> ConsensusResult<bool> {
        if is_current {
            self.current.is_above_threshold(bitmap)
        } else if let Some(auth_list) = self.last.clone() {
            auth_list.is_above_threshold(bitmap)
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last epoch".to_string(),
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
                "There is no authority list cache of last epoch".to_string(),
            ))
        }
    }

    /// Check whether the authority management contains the given address. Return `Err` when
    /// `is_current` is `false`, and the last epoch ID's authority management is `None`.
    pub fn contains(&self, address: &Address, is_current: bool) -> ConsensusResult<bool> {
        if is_current {
            Ok(self.current.contains(address))
        } else if let Some(auth_list) = self.last.clone() {
            Ok(auth_list.contains(address))
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last epoch".to_string(),
            ))
        }
    }

    /// Get the sum of the vote weights. Return `Err` when `is_current` is `false`, and the last
    /// epoch ID's authority management is `None`.
    pub fn get_vote_weight_sum(&self, is_current: bool) -> ConsensusResult<u64> {
        if is_current {
            Ok(self.current.get_vote_weight_sum())
        } else if let Some(auth_list) = self.last.clone() {
            Ok(auth_list.get_vote_weight_sum())
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last epoch".to_string(),
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

/// Epoch authority manage is an extensional data structure of authority list which means
/// `Vec<Node>`. It transforms the information in `Node` struct into a more suitable data structure
/// according to its usage scene. The vote weight need look up by address frequently, therefore,
/// address with vote weight saved in a `HashMap`.
#[display(fmt = "{:?}", address)]
#[derive(Clone, Debug, Display, PartialEq, Eq)]
struct EpochAuthorityManage {
    address:            Vec<Address>,
    propose_weights:    Vec<u64>,
    vote_weight_map:    HashMap<Address, u8>,
    propose_weight_sum: u64,
    vote_weight_sum:    u64,
}

impl EpochAuthorityManage {
    /// Create a new epoch authority manage.
    fn new() -> Self {
        EpochAuthorityManage {
            address:            Vec::new(),
            propose_weights:    Vec::new(),
            vote_weight_map:    HashMap::new(),
            propose_weight_sum: 0u64,
            vote_weight_sum:    0u64,
        }
    }

    /// Update the epoch authority manage by a new authority list.
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
    fn get_vote_weight(&self, addr: &Address) -> ConsensusResult<&u8> {
        self.vote_weight_map
            .get(addr)
            .ok_or_else(|| ConsensusError::InvalidAddress)
    }

    /// Get the proposer address by a given seed.
    fn get_proposer(&self, seed: u64) -> ConsensusResult<Address> {
        let index = get_proposer_index(seed, &self.propose_weights, self.propose_weight_sum);
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

    /// Get the sum of the vote weights in the current epoch ID.
    fn get_vote_weight_sum(&self) -> u64 {
        self.vote_weight_sum
    }

    /// Clear the EpochAuthorityManage, removing all values.
    fn flush(&mut self) {
        self.address.clear();
        self.propose_weights.clear();
        self.vote_weight_map.clear();
        self.propose_weight_sum = 0;
        self.vote_weight_sum = 0;
    }
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
    use crate::utils::auth_manage::{AuthorityManage, EpochAuthorityManage};

    fn gen_address() -> Address {
        Address::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>())
    }

    fn gen_auth_list(len: usize) -> Vec<Node> {
        if len == 0 {
            return vec![];
        }

        let mut authority_list = Vec::new();
        for _ in 0..len {
            authority_list.push(gen_node(gen_address(), random::<u8>(), random::<u8>()));
        }
        authority_list
    }

    fn gen_node(addr: Address, propose_weight: u8, vote_weight: u8) -> Node {
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
        let mut authority_manage = EpochAuthorityManage::new();
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
        let mut e_auth_manage = EpochAuthorityManage::new();

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
            gen_node(gen_address(), 1u8, 1u8),
            gen_node(gen_address(), 1u8, 1u8),
            gen_node(gen_address(), 1u8, 1u8),
            gen_node(gen_address(), 1u8, 1u8),
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
