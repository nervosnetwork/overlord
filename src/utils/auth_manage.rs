use std::collections::HashMap;

use bit_vec::BitVec;
use bytes::Bytes;

use crate::error::ConsensusError;
use crate::types::{Address, Node};
use crate::utils::rand_proposer::get_proposer_index;
use crate::ConsensusResult;

/// Authority manage consits of the current epoch authority manage and the last epoch authority
/// manage The last epoch authority manage which is optional is used to check the correctness of
/// last epoch's message. The last epoch's authority manage should be `Some` unless current epoch is
/// `0` or `1`.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    /// Get a vote weight that correspond to the given address.
    /// **TODO: simplify**
    pub fn get_vote_weight(&self, addr: &Address, is_current: bool) -> ConsensusResult<u8> {
        if is_current {
            self.current.get_vote_weight(addr)
        } else if let Some(auth_list) = self.last.clone() {
            auth_list.get_vote_weight(addr)
        } else {
            Err(ConsensusError::Other(
                "There is no authority list cache of last epoch".to_string(),
            ))
        }
    }

    /// Get a proposer address of the epoch by a given seed.
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

    ///
    pub fn is_above_threshold(&self, bitmap: Bytes, is_current: bool) -> ConsensusResult<bool> {
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

    /// **TODO: add unit test**
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    fn get_vote_weight(&self, addr: &Address) -> ConsensusResult<u8> {
        if let Some(vote_weight) = self.vote_weight_map.get(addr) {
            return Ok(*vote_weight);
        }
        Err(ConsensusError::InvalidAddress)
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

    ///
    fn is_above_threshold(&self, bitmap: Bytes) -> ConsensusResult<bool> {
        let bitmap = BitVec::from_bytes(&bitmap);
        if bitmap.len() != self.address.len() {
            return Err(ConsensusError::AggregatedSignatureErr(
                "Bitmap length error".to_string(),
            ));
        }

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

    /// **TODO: add unit test**
    fn contains(&self, address: &Address) -> bool {
        self.address.contains(address)
    }

    ///
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
    use rand::random;

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
                authority_manage.get_vote_weight(&node.address).unwrap(),
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
}
