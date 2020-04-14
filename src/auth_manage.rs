use std::collections::HashMap;
use std::error::Error;

use bytes::Bytes;
use derive_more::Display;
use rlp::{encode, Encodable};
use serde::export::PhantomData;

use crate::types::{
    Aggregates, Node, PreCommitQC, PreVoteQC, PriKeyHex, Proposal, PubKeyHex, SelectMode,
    SignedPreCommit, SignedPreVote, SignedProposal, Vote, VoteType, Weight,
};
use crate::{Adapter, Address, Blk, CommonHex, Crypto, Hash, Signature, St};

pub struct AuthManage<A: Adapter<B, S>, B: Blk, S: St> {
    common_ref: CommonHex,

    pri_key:        PriKeyHex,
    address:        Address,
    propose_weight: Weight,
    vote_weight:    Weight,

    mode:         SelectMode,
    current_auth: AuthCell,
    last_auth:    Option<AuthCell>,

    phantom_a: PhantomData<A>,
    phantom_b: PhantomData<B>,
    phantom_s: PhantomData<S>,
}

impl<A: Adapter<B, S>, B: Blk, S: St> AuthManage<A, B, S> {
    pub fn update(&mut self, mode: SelectMode, new_auth_list: Vec<Node>) {
        self.mode = mode;
        self.last_auth = Some(self.current_auth.clone());
        if let Some(node) = new_auth_list
            .iter()
            .find(|node| node.address == self.address)
        {
            self.propose_weight = node.propose_weight;
            self.vote_weight = node.vote_weight;
        }
        self.current_auth = AuthCell::new(new_auth_list);
    }

    pub fn sign_proposal(&self, proposal: Proposal<B>) -> Result<SignedProposal<B>, AuthError> {
        let hash = hash::<A, B, Proposal<B>, S>(&proposal);
        let signature = self.sign(&hash)?;
        Ok(SignedProposal::new(proposal, signature))
    }

    pub fn verify_proposal_signature(
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
        let hash = hash_vote::<A, B, S>(&vote, VoteType::PreVote);
        let signature = self.sign(&hash)?;
        Ok(SignedPreVote::new(
            vote,
            self.vote_weight,
            self.address.clone(),
            signature,
        ))
    }

    pub fn verify_pre_vote_signature(
        &self,
        signed_pre_vote: SignedPreVote,
    ) -> Result<(), AuthError> {
        let hash = hash_vote::<A, B, S>(&signed_pre_vote.vote, VoteType::PreVote);
        self.verify_signature(&signed_pre_vote.voter, &hash, &signed_pre_vote.signature)
    }

    pub fn sign_pre_commit(&self, vote: Vote) -> Result<SignedPreCommit, AuthError> {
        let hash = hash_vote::<A, B, S>(&vote, VoteType::PreCommit);
        let signature = self.sign(&hash)?;
        Ok(SignedPreCommit::new(
            vote,
            self.vote_weight,
            self.address.clone(),
            signature,
        ))
    }

    pub fn verify_pre_commit_signature(
        &self,
        signed_pre_commit: SignedPreCommit,
    ) -> Result<(), AuthError> {
        let hash = hash_vote::<A, B, S>(&signed_pre_commit.vote, VoteType::PreCommit);
        self.verify_signature(
            &signed_pre_commit.voter,
            &hash,
            &signed_pre_commit.signature,
        )
    }

    pub fn aggregate_pre_votes(
        &self,
        pre_votes: Vec<SignedPreVote>,
    ) -> Result<PreVoteQC, AuthError> {
        assert!(pre_votes.is_empty());
        let signatures: HashMap<&Address, &Signature> = pre_votes
            .iter()
            .map(|pre_vote| (&pre_vote.voter, &pre_vote.signature))
            .collect();
        let aggregates = self.aggregate(signatures)?;
        Ok(PreVoteQC::new(pre_votes[0].vote.clone(), aggregates))
    }

    pub fn verify_pre_vote_qc_aggregates(&self, pre_vote_qc: PreVoteQC) -> Result<(), AuthError> {
        let hash = hash_vote::<A, B, S>(&pre_vote_qc.vote, VoteType::PreVote);
        self.verify_aggregate(&hash, &pre_vote_qc.aggregates)
    }

    pub fn aggregate_pre_commits(
        &self,
        pre_commits: Vec<SignedPreCommit>,
    ) -> Result<PreCommitQC, AuthError> {
        assert!(pre_commits.is_empty());
        let signatures: HashMap<&Address, &Signature> = pre_commits
            .iter()
            .map(|pre_commit| (&pre_commit.voter, &pre_commit.signature))
            .collect();
        let aggregates = self.aggregate(signatures)?;
        Ok(PreCommitQC::new(pre_commits[0].vote.clone(), aggregates))
    }

    pub fn verify_pre_commit_qc_aggregates(
        &self,
        pre_commit_qc: PreCommitQC,
    ) -> Result<(), AuthError> {
        let hash = hash_vote::<A, B, S>(&pre_commit_qc.vote, VoteType::PreCommit);
        self.verify_aggregate(&hash, &pre_commit_qc.aggregates)
    }

    fn sign(&self, hash: &Hash) -> Result<Signature, AuthError> {
        A::CryptoImpl::sign(self.pri_key.clone(), hash).map_err(AuthError::CryptoErr)
    }

    fn verify_signature(
        &self,
        signer: &Address,
        hash: &Hash,
        signature: &Signature,
    ) -> Result<(), AuthError> {
        let common_ref = self.common_ref.clone();
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
        A::CryptoImpl::aggregate(&self.current_auth.list, signatures).map_err(AuthError::CryptoErr)
    }

    fn verify_aggregate(&self, hash: &Hash, aggregates: &Aggregates) -> Result<(), AuthError> {
        let common_ref = self.common_ref.clone();
        A::CryptoImpl::verify_aggregates(common_ref, &hash, &self.current_auth.list, &aggregates)
            .map_err(AuthError::CryptoErr)
    }
}

fn hash<A: Adapter<B, S>, B: Blk, E: Encodable, S: St>(data: &E) -> Hash {
    let encode = encode(data);
    A::CryptoImpl::hash(&Bytes::from(encode))
}

fn hash_vote<A: Adapter<B, S>, B: Blk, S: St>(vote: &Vote, vote_type: VoteType) -> Hash {
    let mut encode = encode(vote);
    encode.insert(0, vote_type.into());
    A::CryptoImpl::hash(&Bytes::from(encode))
}

#[derive(Clone, Debug)]
struct AuthCell {
    list:               AuthList,
    map:                HashMap<Address, PubKeyHex>,
    vote_weight_sum:    Weight,
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
    #[display(fmt = "crypto error: {}", _0)]
    CryptoErr(Box<dyn Error + Send>),
    #[display(fmt = "unauthorized")]
    UnAuthorized,
}

impl Error for AuthError {}
