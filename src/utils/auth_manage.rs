use std::collections::HashMap;

use crate::error::ConsensusError;
use crate::types::{Address, Node};
use crate::utils::rand_proposer::get_proposer_index;
use crate::ConsensusResult;

/// Authority manage consits of the current epoch authority manage and an old epoch authority manage
/// The old epoch authority manage which is optional is used to check the correctness of old epoch's
/// message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityManage {
    current:       EpochAuthorityManage,
    current_epoch: u64,
    old:           Option<EpochAuthorityManage>,
    old_epoch:     Option<u64>,
}

impl AuthorityManage {
    /// Create a new authority management.
    pub fn new() -> Self {
        AuthorityManage {
            current:       EpochAuthorityManage::new(),
            current_epoch: 0u64,
            old:           None,
            old_epoch:     None,
        }
    }

    /// Update a nwe epoch of authority list, and the old authority will be reserved.
    pub fn update(&mut self, authority_list: Vec<Node>, new_epoch: u64) {
        if !self.current.is_empty() {
            self.old = Some(self.current.clone());
            self.old_epoch = Some(self.current_epoch);
        }
        self.current_epoch = new_epoch;
        self.current.update(authority_list);
    }

    /// Get a vote weight that correspond to the given address and epoch.
    pub fn get_vote_weight(&self, addr: &Address, epoch: u64) -> ConsensusResult<u8> {
        if epoch == self.current_epoch {
            self.current.get_vote_weight(addr)
        } else if Some(epoch) == self.old_epoch {
            self.old.clone().unwrap().get_vote_weight(addr)
        } else {
            Err(ConsensusError::Other(format!(
                "There is no authority list cache of epoch {:?}",
                epoch
            )))
        }
    }

    /// Get a proposer address of the epoch by a given seed.
    pub fn get_proposer(&self, seed: u64, epoch: u64) -> ConsensusResult<Address> {
        if epoch == self.current_epoch {
            self.current.get_proposer(seed)
        } else if Some(epoch) == self.old_epoch {
            self.old.clone().unwrap().get_proposer(seed)
        } else {
            Err(ConsensusError::Other(format!(
                "There is no authority list cache of epoch {:?}",
                epoch
            )))
        }
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
    fn update(&mut self, authority_list: Vec<Node>) {
        self.flush();
        for node in authority_list.into_iter() {
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
            "The address list mismatch proposal weight list".to_string(),
        ))
    }

    /// Clear the EpochAuthorityManage, removing all values.
    fn flush(&mut self) {
        self.address.clear();
        self.propose_weights.clear();
        self.vote_weight_map.clear();
        self.propose_weight_sum = 0;
        self.vote_weight_sum = 0;
    }

    /// If epoch authority manage contains no elements.
    fn is_empty(&self) -> bool {
        self.address.is_empty()
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
        let authority_list = gen_auth_list(0);
        let mut authority_manage = EpochAuthorityManage::new();
        authority_manage.update(authority_list.clone());

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
        authority_manage.update(gen_auth_list(auth_len as usize));

        for node in authority_list.iter() {
            assert_eq!(
                authority_manage.get_vote_weight(&node.address).unwrap(),
                node.propose_weight
            );
        }
    }

    #[test]
    fn test_update() {
        let authority_list = gen_auth_list(random::<u8>() as usize);
        let authority_list_new = gen_auth_list(random::<u8>() as usize);
        let authority_list_newer = gen_auth_list(random::<u8>() as usize);
        let mut auth_manage = AuthorityManage::new();
        let mut e_auth_manage = EpochAuthorityManage::new();

        auth_manage.update(authority_list.clone(), 1);
        e_auth_manage.update(authority_list);
        assert_eq!(auth_manage, AuthorityManage {
            current:       e_auth_manage.clone(),
            current_epoch: 1u64,
            old:           None,
            old_epoch:     None,
        });

        let mut e_auth_manage_old = e_auth_manage.clone();
        auth_manage.update(authority_list_new.clone(), 2);
        e_auth_manage.update(authority_list_new);
        assert_eq!(auth_manage, AuthorityManage {
            current:       e_auth_manage.clone(),
            current_epoch: 2u64,
            old:           Some(e_auth_manage_old),
            old_epoch:     Some(1u64),
        });

        e_auth_manage_old = e_auth_manage.clone();
        auth_manage.update(authority_list_newer.clone(), 3);
        e_auth_manage.update(authority_list_newer);
        assert_eq!(auth_manage, AuthorityManage {
            current:       e_auth_manage,
            current_epoch: 3u64,
            old:           Some(e_auth_manage_old),
            old_epoch:     Some(2u64),
        });
    }
}
