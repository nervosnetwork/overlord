use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::sync::Arc;

use bit_vec::BitVec;
use bytes::Bytes;
use log::warn;
use prime_tools::get_primes_less_than_x;
use rlp::{encode, Encodable};
use tokio::sync::RwLock;

use crate::types::{
    Aggregates, Choke, ChokeQC, FullBlockWithProof, PreCommitQC, PreVoteQC, Proof, Proposal,
    PubKeyHex, SelectMode, SignedChoke, SignedHeight, SignedPreCommit, SignedPreVote,
    SignedProposal, SyncRequest, SyncResponse, UpdateFrom, Vote, VoteType, Weight,
};
use crate::{
    Adapter, Address, AuthConfig, Blk, Crypto, CryptoConfig, FullBlk, Hash, Height, HeightRange,
    PartyPubKeyHex, Round, Signature, St, TinyHex,
};
use crate::{OverlordError, OverlordResult};

pub struct AuthManage<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St> {
    pub crypto_config: CryptoConfig,

    pub current_auth: Arc<RwLock<AuthCell<B>>>,
    pub last_auth:    Arc<RwLock<Option<AuthCell<B>>>>,

    phantom_a: PhantomData<A>,
    phantom_b: PhantomData<B>,
    phantom_f: PhantomData<F>,
    phantom_s: PhantomData<S>,
}

impl<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St> AuthManage<A, B, F, S> {
    pub fn new(
        crypto_config: CryptoConfig,
        current_auth: AuthCell<B>,
        last_auth: Option<AuthCell<B>>,
    ) -> Self {
        AuthManage {
            crypto_config,
            current_auth: Arc::new(RwLock::new(current_auth)),
            last_auth: Arc::new(RwLock::new(last_auth)),

            phantom_a: PhantomData,
            phantom_b: PhantomData,
            phantom_f: PhantomData,
            phantom_s: PhantomData,
        }
    }

    pub async fn handle_commit(&self, config: AuthConfig) {
        assert_eq!(
            self.crypto_config.common_ref, config.common_ref,
            "CommonRef mismatch, run in wrong chain!"
        );
        let mut current_auth = self.current_auth.write().await;

        *self.last_auth.write().await = Some(current_auth.clone());
        *current_auth = AuthCell::new(config, &self.crypto_config.address);
    }

    pub async fn sign_proposal(&self, proposal: Proposal<B>) -> OverlordResult<SignedProposal<B>> {
        let hash = hash::<A, B, Proposal<B>, F, S>(&proposal);
        let signature = self.party_sign(&hash, true).await?;
        Ok(SignedProposal::new(proposal, signature))
    }

    pub async fn verify_signed_proposal(&self, sp: &SignedProposal<B>) -> OverlordResult<()> {
        self.check_leader(sp.proposal.height, sp.proposal.round, &sp.proposal.proposer)
            .await?;
        let hash = hash::<A, B, Proposal<B>, F, S>(&sp.proposal);
        self.party_verify_signature(&sp.proposal.proposer, &hash, &sp.signature)
            .await
    }

    pub async fn sign_pre_vote(&self, vote: Vote) -> OverlordResult<SignedPreVote> {
        let hash = hash_vote::<A, B, Vote, F, S>(&vote, VoteType::PreVote);
        let signature = self.party_sign(&hash, true).await?;
        Ok(SignedPreVote::new(
            vote,
            self.current_auth.read().await.vote_weight,
            self.crypto_config.address.clone(),
            signature,
        ))
    }

    pub async fn verify_signed_pre_vote(&self, signed_vote: &SignedPreVote) -> OverlordResult<()> {
        self.current_auth
            .read()
            .await
            .check_vote_weight(&signed_vote.voter, signed_vote.vote_weight)?;
        let hash = hash_vote::<A, B, Vote, F, S>(&signed_vote.vote, VoteType::PreVote);
        self.party_verify_signature(&signed_vote.voter, &hash, &signed_vote.signature)
            .await
    }

    pub async fn sign_pre_commit(&self, vote: Vote) -> OverlordResult<SignedPreCommit> {
        let hash = hash_vote::<A, B, Vote, F, S>(&vote, VoteType::PreCommit);
        let signature = self.party_sign(&hash, true).await?;
        Ok(SignedPreCommit::new(
            vote,
            self.current_auth.read().await.vote_weight,
            self.crypto_config.address.clone(),
            signature,
        ))
    }

    pub async fn verify_signed_pre_commit(
        &self,
        signed_vote: &SignedPreCommit,
    ) -> OverlordResult<()> {
        self.current_auth
            .read()
            .await
            .check_vote_weight(&signed_vote.voter, signed_vote.vote_weight)?;
        let hash = hash_vote::<A, B, Vote, F, S>(&signed_vote.vote, VoteType::PreCommit);
        self.party_verify_signature(&signed_vote.voter, &hash, &signed_vote.signature)
            .await
    }

    pub async fn aggregate_pre_votes(
        &self,
        pre_votes: Vec<SignedPreVote>,
    ) -> OverlordResult<PreVoteQC> {
        let mut pair_list = vec![];
        let mut signatures = HashMap::new();
        pre_votes.iter().for_each(|pre_vote| {
            pair_list.push((&pre_vote.voter, pre_vote.vote_weight));
            signatures.insert(&pre_vote.voter, &pre_vote.signature);
        });

        self.current_auth
            .read()
            .await
            .ensure_majority_weight(pair_list)?;
        let aggregates = self.aggregate(signatures).await?;
        Ok(PreVoteQC::new(pre_votes[0].vote.clone(), aggregates))
    }

    pub async fn verify_pre_vote_qc(&self, pre_vote_qc: &PreVoteQC) -> OverlordResult<()> {
        let hash = hash_vote::<A, B, Vote, F, S>(&pre_vote_qc.vote, VoteType::PreVote);
        self.verify_aggregate(&hash, &pre_vote_qc.aggregates).await
    }

    pub async fn aggregate_pre_commits(
        &self,
        pre_commits: Vec<SignedPreCommit>,
    ) -> OverlordResult<PreCommitQC> {
        let mut pair_list = vec![];
        let mut signatures = HashMap::new();
        pre_commits.iter().for_each(|pre_commit| {
            pair_list.push((&pre_commit.voter, pre_commit.vote_weight));
            signatures.insert(&pre_commit.voter, &pre_commit.signature);
        });

        self.current_auth
            .read()
            .await
            .ensure_majority_weight(pair_list)?;
        let aggregates = self.aggregate(signatures).await?;
        Ok(PreCommitQC::new(pre_commits[0].vote.clone(), aggregates))
    }

    pub async fn verify_pre_commit_qc(&self, pre_commit_qc: &PreCommitQC) -> OverlordResult<()> {
        let hash = hash_vote::<A, B, Vote, F, S>(&pre_commit_qc.vote, VoteType::PreCommit);
        self.verify_aggregate(&hash, &pre_commit_qc.aggregates)
            .await
    }

    pub async fn sign_choke(
        &self,
        choke: Choke,
        from: Option<UpdateFrom>,
    ) -> OverlordResult<SignedChoke> {
        let hash = hash_vote::<A, B, Choke, F, S>(&choke, VoteType::Choke);
        let signature = self.party_sign(&hash, true).await?;
        Ok(SignedChoke::new(
            choke,
            self.current_auth.read().await.vote_weight,
            from,
            self.crypto_config.address.clone(),
            signature,
        ))
    }

    pub async fn verify_signed_choke(&self, signed_choke: &SignedChoke) -> OverlordResult<()> {
        self.current_auth
            .read()
            .await
            .check_vote_weight(&signed_choke.voter, signed_choke.vote_weight)?;
        let hash = hash_vote::<A, B, Choke, F, S>(&signed_choke.choke, VoteType::Choke);
        self.party_verify_signature(&signed_choke.voter, &hash, &signed_choke.signature)
            .await
    }

    pub async fn aggregate_chokes(&self, chokes: Vec<SignedChoke>) -> OverlordResult<ChokeQC> {
        assert!(
            !chokes.is_empty(),
            "Unreachable! chokes is empty while aggregate chokes!"
        );

        let mut pair_list = vec![];
        let mut signatures = HashMap::new();
        chokes.iter().for_each(|choke| {
            pair_list.push((&choke.voter, choke.vote_weight));
            signatures.insert(&choke.voter, &choke.signature);
        });

        self.current_auth
            .read()
            .await
            .ensure_majority_weight(pair_list)?;
        let aggregates = self.aggregate(signatures).await?;
        Ok(ChokeQC::new(chokes[0].choke.clone(), aggregates))
    }

    pub async fn verify_choke_qc(&self, choke_qc: &ChokeQC) -> OverlordResult<()> {
        let hash = hash_vote::<A, B, Choke, F, S>(&choke_qc.choke, VoteType::Choke);
        self.verify_aggregate(&hash, &choke_qc.aggregates).await
    }

    pub async fn verify_proof(&self, proof: Proof) -> OverlordResult<()> {
        let hash = hash_vote::<A, B, Vote, F, S>(&proof.vote, VoteType::PreCommit);

        if let Some(last_auth) = &*self.last_auth.read().await {
            let common_ref = self.crypto_config.common_ref.clone();
            let voters = last_auth.get_voters(&proof.aggregates.address_bitmap);
            let party_pub_keys = last_auth.get_party_pub_keys(voters.as_ref());
            last_auth.ensure_majority(voters)?;
            A::CryptoImpl::verify_aggregates(
                common_ref,
                &hash,
                party_pub_keys.as_slice(),
                &proof.aggregates.signature,
            )
            .map_err(OverlordError::byz_crypto)
        } else {
            warn!("verify proof of height 0, which will always pass");
            Ok(())
        }
    }

    pub fn sign_height(&self, height: Height) -> OverlordResult<SignedHeight> {
        let height_vec = height.to_be_bytes()[0..].to_vec();
        let hash = A::CryptoImpl::hash(&Bytes::from(height_vec));
        let signature = self.sign(&hash)?;
        Ok(SignedHeight::new(
            height,
            self.crypto_config.address.clone(),
            self.crypto_config.pub_key.clone(),
            signature,
        ))
    }

    pub fn verify_signed_height(&self, signed_height: &SignedHeight) -> OverlordResult<()> {
        let height_vec = signed_height.height.to_be_bytes()[0..].to_vec();
        let hash = A::CryptoImpl::hash(&Bytes::from(height_vec));
        self.verify_signature(
            signed_height.pub_key_hex.clone(),
            &signed_height.address,
            &hash,
            &signed_height.signature,
        )
    }

    pub fn sign_sync_request(&self, range: HeightRange) -> OverlordResult<SyncRequest> {
        let hash = A::CryptoImpl::hash(&Bytes::from(rlp::encode(&range)));
        let signature = self.sign(&hash)?;
        Ok(SyncRequest::new(
            range,
            self.crypto_config.address.clone(),
            self.crypto_config.pub_key.clone(),
            signature,
        ))
    }

    pub fn verify_sync_request(&self, request: &SyncRequest) -> OverlordResult<()> {
        let hash = A::CryptoImpl::hash(&Bytes::from(rlp::encode(&request.request_range)));
        self.verify_signature(
            request.pub_key_hex.clone(),
            &request.requester,
            &hash,
            &request.signature,
        )
    }

    pub fn sign_sync_response(
        &self,
        range: HeightRange,
        block_with_proofs: Vec<FullBlockWithProof<B, F>>,
    ) -> OverlordResult<SyncResponse<B, F>> {
        let hash = A::CryptoImpl::hash(&Bytes::from(rlp::encode(&range)));
        let signature = self.sign(&hash)?;
        Ok(SyncResponse::new(
            range,
            block_with_proofs,
            self.crypto_config.address.clone(),
            self.crypto_config.pub_key.clone(),
            signature,
        ))
    }

    pub fn verify_sync_response(&self, response: &SyncResponse<B, F>) -> OverlordResult<()> {
        let hash = A::CryptoImpl::hash(&Bytes::from(rlp::encode(&response.request_range)));
        self.verify_signature(
            response.pub_key_hex.clone(),
            &response.responder,
            &hash,
            &response.signature,
        )
    }

    pub async fn get_leader(&self, height: Height, round: Round) -> Address {
        self.current_auth
            .read()
            .await
            .calculate_leader(height, round)
    }

    pub async fn is_auth(&self) -> OverlordResult<()> {
        if self.current_auth.read().await.vote_weight == 0 {
            return Err(OverlordError::debug_un_auth());
        }
        Ok(())
    }

    async fn party_sign(&self, hash: &Hash, need_auth: bool) -> OverlordResult<Signature> {
        if need_auth {
            self.is_auth().await?;
        }
        A::CryptoImpl::party_sign_msg(self.crypto_config.pri_key.clone(), hash)
            .map_err(OverlordError::local_crypto)
    }

    fn sign(&self, hash: &Hash) -> OverlordResult<Signature> {
        A::CryptoImpl::sign_msg(self.crypto_config.pri_key.clone(), hash)
            .map_err(OverlordError::local_crypto)
    }

    async fn party_verify_signature(
        &self,
        signer: &Address,
        hash: &Hash,
        signature: &Signature,
    ) -> OverlordResult<()> {
        let common_ref = self.crypto_config.common_ref.clone();
        let current_auth = self.current_auth.read().await;
        let party_pub_key = current_auth
            .map
            .get(signer)
            .ok_or_else(OverlordError::byz_un_auth)?;
        A::CryptoImpl::party_verify_signature(
            common_ref,
            party_pub_key.to_string(),
            hash,
            signature,
        )
        .map_err(OverlordError::byz_crypto)
    }

    fn verify_signature(
        &self,
        pub_key: PubKeyHex,
        signer: &Address,
        hash: &Hash,
        signature: &Signature,
    ) -> OverlordResult<()> {
        A::CryptoImpl::verify_signature(pub_key, signer, hash, signature)
            .map_err(OverlordError::byz_crypto)
    }

    async fn aggregate(
        &self,
        signatures: HashMap<&Address, &Signature>,
    ) -> OverlordResult<Aggregates> {
        let current_auth = self.current_auth.read().await;

        let bitmap = current_auth.gen_bit_map(signatures.keys().cloned().collect());
        let signatures = current_auth.replace_party_pub_keys(signatures);
        let signature = A::CryptoImpl::aggregate(signatures.iter().collect())
            .map_err(OverlordError::local_crypto)?;
        Ok(Aggregates::new(bitmap, signature))
    }

    async fn verify_aggregate(&self, hash: &Hash, aggregates: &Aggregates) -> OverlordResult<()> {
        let current_auth = self.current_auth.read().await;

        let common_ref = self.crypto_config.common_ref.clone();
        let voters = current_auth.get_voters(&aggregates.address_bitmap);
        let party_pub_keys = current_auth.get_party_pub_keys(voters.as_ref());
        current_auth.ensure_majority(voters)?;
        A::CryptoImpl::verify_aggregates(common_ref, &hash, &party_pub_keys, &aggregates.signature)
            .map_err(OverlordError::byz_crypto)
    }

    async fn check_leader(
        &self,
        height: Height,
        round: Round,
        leader: &Address,
    ) -> OverlordResult<()> {
        let expect_leader = self.get_leader(height, round).await;
        if leader != &expect_leader {
            return Err(OverlordError::byz_leader(format!(
                "msg.leader != exact.leader, {} != {}",
                leader.tiny_hex(),
                expect_leader.tiny_hex()
            )));
        }
        Ok(())
    }
}

fn hash<A: Adapter<B, F, S>, B: Blk, E: Encodable, F: FullBlk<B>, S: St>(data: &E) -> Hash {
    let encode = encode(data);
    A::CryptoImpl::hash(&Bytes::from(encode))
}

fn hash_vote<A: Adapter<B, F, S>, B: Blk, E: Encodable, F: FullBlk<B>, S: St>(
    data: &E,
    vote_type: VoteType,
) -> Hash {
    let mut encode = encode(data);
    encode.insert(0, vote_type.into());
    A::CryptoImpl::hash(&Bytes::from(encode))
}

#[derive(Clone, Debug, Default)]
pub struct AuthCell<B: Blk> {
    pub mode:               SelectMode,
    pub vote_weight:        Weight,
    pub propose_weight:     Weight,
    pub vote_weight_sum:    Weight,
    pub propose_weight_sum: Weight,

    pub list:            AuthList,
    pub map:             HashMap<Address, PartyPubKeyHex>,
    pub vote_weight_map: HashMap<Address, Weight>,

    pub phantom: PhantomData<B>,
}

impl<B: Blk> AuthCell<B> {
    pub fn new(config: AuthConfig, my_address: &Address) -> Self {
        let mut list = vec![];
        let mut vote_weight_map = HashMap::new();
        let mut vote_weight_sum = 0;
        let mut propose_weight_sum = 0;

        config.auth_list.iter().for_each(|node| {
            list.push((node.address.clone(), node.party_pub_key.clone()));
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
            phantom: PhantomData,
        }
    }

    fn calculate_leader(&self, height: Height, round: Round) -> Address {
        // Todo: add random mode
        let len = self.list.len();
        let prime_num = *get_primes_less_than_x(len as u32).last().unwrap_or(&1) as u64;
        let index = (height * prime_num + round) % (len as u64);
        self.list
            .get(index as usize)
            .expect("Unreachable! Calculate a leader index out of auth_list")
            .0
            .clone()
    }

    fn replace_party_pub_keys(
        &self,
        signatures: HashMap<&Address, &Signature>,
    ) -> HashMap<PartyPubKeyHex, Signature> {
        let mut map = HashMap::new();
        for (address, signature) in signatures {
            if let Some(pub_key) = self.map.get(address) {
                map.insert(pub_key.clone(), signature.clone());
            }
        }
        map
    }

    fn get_party_pub_keys(&self, address_list: &[Address]) -> Vec<PartyPubKeyHex> {
        let mut vec = Vec::new();
        for address in address_list {
            if let Some(pub_key) = self.map.get(address) {
                vec.push(pub_key.clone())
            }
        }
        vec
    }

    fn check_vote_weight(&self, address: &Address, vote_weight: Weight) -> OverlordResult<()> {
        let expect_weight = self
            .vote_weight_map
            .get(address)
            .ok_or_else(OverlordError::byz_un_auth)?;
        if expect_weight != &vote_weight {
            return Err(OverlordError::byz_fake(format!(
                "msg.weight != exact.weight, {} != {}",
                vote_weight, expect_weight
            )));
        }
        Ok(())
    }

    fn ensure_majority_weight(&self, pair_list: Vec<(&Address, Weight)>) -> OverlordResult<()> {
        let mut weight_sum = 0;
        for (address, weight) in pair_list {
            self.check_vote_weight(address, weight)?;
            weight_sum += weight;
        }
        if !self.beyond_majority(weight_sum) {
            return Err(OverlordError::byz_under_maj());
        }
        Ok(())
    }

    fn ensure_majority(&self, list: Vec<Address>) -> OverlordResult<()> {
        let mut weight_sum = 0;
        for address in list.iter() {
            weight_sum += *self
                .vote_weight_map
                .get(address)
                .ok_or_else(OverlordError::byz_un_auth)?;
        }
        if !self.beyond_majority(weight_sum) {
            return Err(OverlordError::byz_under_maj());
        }
        Ok(())
    }

    pub fn beyond_majority(&self, weight_sum: Weight) -> bool {
        weight_sum * 3 > self.vote_weight_sum * 2
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

pub type AuthList = Vec<(Address, PartyPubKeyHex)>;
