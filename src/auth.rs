#![allow(unused_imports)]

use std::collections::{HashMap, HashSet};
use std::error::Error;

use bit_vec::BitVec;
use bytes::Bytes;
use derive_more::Display;
use log::warn;
use rlp::{encode, Encodable};
use serde::export::PhantomData;

use crate::types::{
    Aggregates, Choke, Node, PreCommitQC, PreVoteQC, PriKeyHex, Proof, Proposal, PubKeyHex,
    SelectMode, SignedChoke, SignedPreCommit, SignedPreVote, SignedProposal, UpdateFrom, Vote,
    VoteType, Weight,
};
use crate::{Adapter, Address, AuthConfig, Blk, CommonHex, Crypto, Hash, Signature, St};

pub struct AuthManage<A: Adapter<B, S>, B: Blk, S: St> {
    fixed_config: AuthFixedConfig,

    current_auth: AuthCell,
    last_auth:    Option<AuthCell>,

    phantom_a: PhantomData<A>,
    phantom_b: PhantomData<B>,
    phantom_s: PhantomData<S>,
}

impl<A: Adapter<B, S>, B: Blk, S: St> AuthManage<A, B, S> {
    pub fn new(
        fixed_config: AuthFixedConfig,
        current_auth: AuthCell,
        last_auth: Option<AuthCell>,
    ) -> Self {
        AuthManage {
            fixed_config,
            current_auth,
            last_auth,

            phantom_a: PhantomData,
            phantom_b: PhantomData,
            phantom_s: PhantomData,
        }
    }

    pub fn update(&mut self, config: AuthConfig) {
        assert_eq!(
            self.fixed_config.common_ref, config.common_ref,
            "CommonRef mismatch, run in wrong chain!"
        );
        self.last_auth = Some(self.current_auth.clone());
        self.current_auth = AuthCell::new(config, &self.fixed_config.address);
    }

    pub fn sign_proposal(&self, proposal: Proposal<B>) -> Result<SignedProposal<B>, AuthError> {
        let hash = hash::<A, B, Proposal<B>, S>(&proposal);
        let signature = self.sign(&hash)?;
        Ok(SignedProposal::new(proposal, signature))
    }

    pub fn verify_signed_proposal(
        &self,
        signed_proposal: SignedProposal<B>,
    ) -> Result<(), AuthError> {
        let hash = hash::<A, B, Proposal<B>, S>(&signed_proposal.proposal);
        self.verify_signature(
            &signed_proposal.proposal.proposer,
            &hash,
            &signed_proposal.signature,
        )
    }

    pub fn sign_pre_vote(&self, vote: Vote) -> Result<SignedPreVote, AuthError> {
        let hash = hash_vote::<A, B, Vote, S>(&vote, VoteType::PreVote);
        let signature = self.sign(&hash)?;
        Ok(SignedPreVote::new(
            vote,
            self.current_auth.vote_weight,
            self.fixed_config.address.clone(),
            signature,
        ))
    }

    pub fn verify_signed_pre_vote(&self, signed_vote: SignedPreVote) -> Result<(), AuthError> {
        self.current_auth
            .check_vote_weight(&signed_vote.voter, signed_vote.vote_weight)?;
        let hash = hash_vote::<A, B, Vote, S>(&signed_vote.vote, VoteType::PreVote);
        self.verify_signature(&signed_vote.voter, &hash, &signed_vote.signature)
    }

    pub fn sign_pre_commit(&self, vote: Vote) -> Result<SignedPreCommit, AuthError> {
        let hash = hash_vote::<A, B, Vote, S>(&vote, VoteType::PreCommit);
        let signature = self.sign(&hash)?;
        Ok(SignedPreCommit::new(
            vote,
            self.current_auth.vote_weight,
            self.fixed_config.address.clone(),
            signature,
        ))
    }

    pub fn verify_signed_pre_commit(&self, signed_vote: SignedPreCommit) -> Result<(), AuthError> {
        self.current_auth
            .check_vote_weight(&signed_vote.voter, signed_vote.vote_weight)?;
        let hash = hash_vote::<A, B, Vote, S>(&signed_vote.vote, VoteType::PreCommit);
        self.verify_signature(&signed_vote.voter, &hash, &signed_vote.signature)
    }

    pub fn aggregate_pre_votes(
        &self,
        pre_votes: Vec<SignedPreVote>,
    ) -> Result<PreVoteQC, AuthError> {
        assert!(
            pre_votes.is_empty(),
            "pre_votes is empty which won't happen!"
        );

        let mut pair_list = vec![];
        let mut signatures = HashMap::new();
        pre_votes.iter().for_each(|pre_vote| {
            pair_list.push((&pre_vote.voter, pre_vote.vote_weight));
            signatures.insert(&pre_vote.voter, &pre_vote.signature);
        });

        self.current_auth.ensure_majority_weight(pair_list)?;
        let aggregates = self.aggregate(signatures)?;
        Ok(PreVoteQC::new(pre_votes[0].vote.clone(), aggregates))
    }

    pub fn verify_pre_vote_qc(&self, pre_vote_qc: PreVoteQC) -> Result<(), AuthError> {
        let hash = hash_vote::<A, B, Vote, S>(&pre_vote_qc.vote, VoteType::PreVote);
        self.verify_aggregate(&hash, &pre_vote_qc.aggregates)
    }

    pub fn aggregate_pre_commits(
        &self,
        pre_commits: Vec<SignedPreCommit>,
    ) -> Result<PreCommitQC, AuthError> {
        assert!(
            pre_commits.is_empty(),
            "pre_commits is empty which won't happen!"
        );

        let mut pair_list = vec![];
        let mut signatures = HashMap::new();
        pre_commits.iter().for_each(|pre_commit| {
            pair_list.push((&pre_commit.voter, pre_commit.vote_weight));
            signatures.insert(&pre_commit.voter, &pre_commit.signature);
        });

        self.current_auth.ensure_majority_weight(pair_list)?;
        let aggregates = self.aggregate(signatures)?;
        Ok(PreCommitQC::new(pre_commits[0].vote.clone(), aggregates))
    }

    pub fn verify_pre_commit_qc(&self, pre_commit_qc: PreCommitQC) -> Result<(), AuthError> {
        let hash = hash_vote::<A, B, Vote, S>(&pre_commit_qc.vote, VoteType::PreCommit);
        self.verify_aggregate(&hash, &pre_commit_qc.aggregates)
    }

    pub fn sign_choke(&self, choke: Choke, from: UpdateFrom) -> Result<SignedChoke, AuthError> {
        let hash = hash_vote::<A, B, Choke, S>(&choke, VoteType::Choke);
        let signature = self.sign(&hash)?;
        Ok(SignedChoke::new(
            choke,
            self.current_auth.vote_weight,
            from,
            self.fixed_config.address.clone(),
            signature,
        ))
    }

    pub fn verify_signed_choke(&self, signed_choke: SignedChoke) -> Result<(), AuthError> {
        self.current_auth
            .check_vote_weight(&signed_choke.voter, signed_choke.vote_weight)?;
        let hash = hash_vote::<A, B, Choke, S>(&signed_choke.choke, VoteType::Choke);
        self.verify_signature(&signed_choke.voter, &hash, &signed_choke.signature)
    }

    pub fn verify_proof(&self, proof: Proof) -> Result<(), AuthError> {
        let hash = hash_vote::<A, B, Vote, S>(&proof.vote, VoteType::PreCommit);

        if let Some(last_auth) = &self.last_auth {
            let common_ref = self.fixed_config.common_ref.clone();
            let voters = last_auth.get_voters(&proof.aggregates.address_bitmap);
            let pub_keys = last_auth.get_pub_keys(voters.as_ref());
            last_auth.ensure_majority(voters)?;
            A::CryptoImpl::verify_aggregates(
                common_ref,
                &hash,
                pub_keys.as_slice(),
                &proof.aggregates.signature,
            )
            .map_err(AuthError::CryptoErr)
        } else {
            warn!("verify proof of height 0, which will always pass");
            Ok(())
        }
    }

    fn sign(&self, hash: &Hash) -> Result<Signature, AuthError> {
        A::CryptoImpl::sign(self.fixed_config.pri_key.clone(), hash).map_err(AuthError::CryptoErr)
    }

    fn verify_signature(
        &self,
        signer: &Address,
        hash: &Hash,
        signature: &Signature,
    ) -> Result<(), AuthError> {
        let common_ref = self.fixed_config.common_ref.clone();
        let pub_key = self
            .current_auth
            .map
            .get(signer)
            .ok_or(AuthError::UnAuthorized)?;
        A::CryptoImpl::verify_signature(common_ref, pub_key.to_string(), hash, signature)
            .map_err(AuthError::CryptoErr)
    }

    fn aggregate(
        &self,
        signatures: HashMap<&Address, &Signature>,
    ) -> Result<Aggregates, AuthError> {
        let bitmap = self
            .current_auth
            .gen_bit_map(signatures.keys().cloned().collect());
        let signatures = self.current_auth.replace_pub_keys(signatures);
        let signature =
            A::CryptoImpl::aggregate(signatures.iter().collect()).map_err(AuthError::CryptoErr)?;
        Ok(Aggregates::new(bitmap, signature))
    }

    fn verify_aggregate(&self, hash: &Hash, aggregates: &Aggregates) -> Result<(), AuthError> {
        let common_ref = self.fixed_config.common_ref.clone();
        let voters = self.current_auth.get_voters(&aggregates.address_bitmap);
        let pub_keys = self.current_auth.get_pub_keys(voters.as_ref());
        self.current_auth.ensure_majority(voters)?;
        A::CryptoImpl::verify_aggregates(common_ref, &hash, &pub_keys, &aggregates.signature)
            .map_err(AuthError::CryptoErr)
    }
}

fn hash<A: Adapter<B, S>, B: Blk, E: Encodable, S: St>(data: &E) -> Hash {
    let encode = encode(data);
    A::CryptoImpl::hash(&Bytes::from(encode))
}

fn hash_vote<A: Adapter<B, S>, B: Blk, E: Encodable, S: St>(data: &E, vote_type: VoteType) -> Hash {
    let mut encode = encode(data);
    encode.insert(0, vote_type.into());
    A::CryptoImpl::hash(&Bytes::from(encode))
}

#[derive(Clone, Debug, Default)]
pub struct AuthCell {
    pub mode:               SelectMode,
    pub vote_weight:        Weight,
    pub propose_weight:     Weight,
    pub vote_weight_sum:    Weight,
    pub propose_weight_sum: Weight,

    pub list:            AuthList,
    pub map:             HashMap<Address, PubKeyHex>,
    pub vote_weight_map: HashMap<Address, Weight>,
}

impl AuthCell {
    pub fn new(config: AuthConfig, my_address: &Address) -> Self {
        let mut list = vec![];
        let mut vote_weight_map = HashMap::new();
        let mut vote_weight_sum = 0;
        let mut propose_weight_sum = 0;

        config.auth_list.iter().for_each(|node| {
            list.push((node.address.clone(), node.pub_key.clone()));
            vote_weight_map.insert(node.address.clone(), node.vote_weight);
            vote_weight_sum += node.vote_weight;
            propose_weight_sum += node.propose_weight;
        });
        let map = list.clone().into_iter().collect();

        let (vote_weight, propose_weight) = config
            .auth_list
            .iter()
            .find(|node| node.address == *my_address)
            .map_or((0, 0), |node| (node.vote_weight, node.propose_weight));

        Self {
            mode: config.mode,
            list,
            map,
            vote_weight,
            propose_weight,
            vote_weight_map,
            vote_weight_sum,
            propose_weight_sum,
        }
    }

    fn replace_pub_keys(
        &self,
        signatures: HashMap<&Address, &Signature>,
    ) -> HashMap<PubKeyHex, Signature> {
        let mut map = HashMap::new();
        for (address, signature) in signatures {
            if let Some(pub_key) = self.map.get(address) {
                map.insert(pub_key.clone(), signature.clone());
            }
        }
        map
    }

    fn get_pub_keys(&self, address_list: &[Address]) -> Vec<PubKeyHex> {
        let mut vec = Vec::new();
        for address in address_list {
            if let Some(pub_key) = self.map.get(address) {
                vec.push(pub_key.clone())
            }
        }
        vec
    }

    fn check_vote_weight(&self, address: &Address, vote_weight: Weight) -> Result<(), AuthError> {
        let expect_weight = self
            .vote_weight_map
            .get(address)
            .ok_or(AuthError::UnAuthorized)?;
        if expect_weight != &vote_weight {
            return Err(AuthError::FakeVoteWeight);
        }
        Ok(())
    }

    fn ensure_majority_weight(&self, pair_list: Vec<(&Address, Weight)>) -> Result<(), AuthError> {
        let mut weight_sum = 0;
        for (address, weight) in pair_list {
            self.check_vote_weight(address, weight)?;
            weight_sum += weight;
        }
        self.check_majority(weight_sum)
    }

    fn ensure_majority(&self, list: Vec<Address>) -> Result<(), AuthError> {
        let mut weight_sum = 0;
        for address in list.iter() {
            weight_sum += *self
                .vote_weight_map
                .get(address)
                .ok_or(AuthError::UnAuthorized)?;
        }
        self.check_majority(weight_sum)
    }

    fn check_majority(&self, weight_sum: Weight) -> Result<(), AuthError> {
        if weight_sum * 3 < self.vote_weight_sum * 2 {
            return Err(AuthError::BelowMajority);
        }
        Ok(())
    }

    fn gen_bit_map(&self, set: HashSet<&Address>) -> Bytes {
        let mut bit_map = BitVec::from_elem(self.list.len(), false);
        for (index, (address, _)) in self.list.iter().enumerate() {
            if set.contains(&address) {
                bit_map.set(index, true);
            }
        }
        Bytes::from(bit_map.to_bytes())
    }

    fn get_voters(&self, bitmap: &[u8]) -> Vec<Address> {
        let bitmap = BitVec::from_bytes(bitmap);
        bitmap
            .iter()
            .zip(self.list.iter())
            .filter(|node| node.0)
            .map(|node| (node.1).0.clone())
            .collect()
    }
}

pub struct AuthFixedConfig {
    pub common_ref: CommonHex,
    pub address:    Address,
    pri_key:        PriKeyHex,
}

impl AuthFixedConfig {
    pub fn new(common_ref: CommonHex, pri_key: PriKeyHex, address: Address) -> Self {
        AuthFixedConfig {
            common_ref,
            pri_key,
            address,
        }
    }
}

pub type AuthList = Vec<(Address, PubKeyHex)>;

#[derive(Debug, Display)]
pub enum AuthError {
    #[display(fmt = "crypto error: {}", _0)]
    CryptoErr(Box<dyn Error + Send>),
    #[display(fmt = "unauthorized address")]
    UnAuthorized,
    #[display(fmt = "fake vote_weight")]
    FakeVoteWeight,
    #[display(fmt = "the sum of vote weights below majority")]
    BelowMajority,
}

impl Error for AuthError {}
