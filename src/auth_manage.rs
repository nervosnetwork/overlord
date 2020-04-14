use std::error::Error;
use std::collections::HashMap;

use bincode::{serialize, deserialize, Error as BinError};
use derive_more::Display;
use serde::export::PhantomData;

use crate::types::{PriKeyHex, PubKeyHex, Height, Weight, Node, SelectMode, Vote, Proposal, SignedProposal, SignedPreVote, PreCommitQC, SignedPreCommit, PreVoteQC, SignedHeight};
use crate::{Address, Blk, Crypto, Adapter, St};

pub struct AuthManage<A: Adapter<B, S>, B: Blk, S: St> {
    pri_key: PriKeyHex,
    address: Address,
    mode: SelectMode,
    current_auth: AuthCell,
    last_auth: Option<AuthCell>,
    phantom_a: PhantomData<A>,
    phantom_b: PhantomData<B>,
    phantom_s: PhantomData<S>,
}

impl<A: Adapter<B, S>, B: Blk, S: St> AuthManage<A, B, S> {
    fn new(pri_key: PriKeyHex, address: Address, mode: SelectMode, current_auth_list: Vec<Node>, last_auth_list: Option<Vec<Node>>) -> Self {
        let current_auth = AuthCell::new(current_auth_list);
        let last_auth = last_auth_list.map(AuthCell::new);
        let phantom_a = PhantomData;
        let phantom_b = PhantomData;
        let phantom_s = PhantomData;
        AuthManage{
            pri_key,
            address,
            mode,
            current_auth,
            last_auth,
            phantom_a,
            phantom_b,
            phantom_s,
        }
    }

    fn update(&mut self, mode: SelectMode, new_auth_list: Vec<Node>) {
        self.mode = mode;
        self.last_auth = Some(self.current_auth.clone());
        self.current_auth = AuthCell::new(new_auth_list);
    }

    fn sign_proposal(&self, proposal: Proposal<B>) -> Result<(), AuthError> {
        let encode = serialize(&proposal).map_err(AuthError::SerializeFailed)?;
        let hash = A::CryptoImpl::hash(&encode);
        // let signature = A::CryptoImpl
        Ok(())
    }

    // fn sign_pre_vote(&self, vote: Vote) -> Result<SignedPreVote, AuthError> {
    //
    // }
    //
    // fn sign_pre_commit(&self, vote: Vote) -> Result<SignedPreCommit, AuthError> {
    //
    // }
    //
    // fn aggregate_pre_votes(&self, pre_votes: Vec<SignedPreVote>) -> Result<PreVoteQC, AuthError> {
    //
    // }
    //
    // fn aggregate_pre_commit(&self, pre_commits: Vec<SignedPreCommit>) -> Result<PreCommitQC, AuthError> {
    //
    // }
}

#[derive(Debug)]
struct AuthCell {
    list: AuthList,
    map: HashMap<Address, PubKeyHex>,
    vote_weight_sum: Weight,
    propose_weight_sum: Weight,
}

impl AuthCell {
    fn new(auth_list: Vec<Node>) -> Self {
        let mut list = vec![];
        let mut vote_weight_sum = 0;
        let mut propose_weight_sum = 0;

        auth_list.iter().for_each(|node| {
            list.push((node.address.clone(), node.pub_key.clone()));
            vote_weight_sum += node.vote_weight;
            propose_weight_sum += node.propose_weight;
        });
        let map = list.clone().into_iter().collect();

        Self {
            list,
            map,
            vote_weight_sum,
            propose_weight_sum,
        }
    }
}

pub type AuthList = Vec<(Address, PubKeyHex)>;

#[derive(Debug, Display)]
pub enum AuthError {
    #[display(fmt = "Serialize failed {:?}", _0)]
    SerializeFailed(BinError),

}

impl Error for AuthError {}