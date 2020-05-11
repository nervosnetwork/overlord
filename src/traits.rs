pub use creep::Context;

use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display};

use async_trait::async_trait;
use bytes::Bytes;
use futures::channel::mpsc::UnboundedSender;

use crate::crypto::PubKeyHex;
use crate::error::OverlordError;
use crate::types::{
    Address, BlockState, CommonHex, ExecResult, FullBlockWithProof, Hash, Height, HeightRange,
    OverlordMsg, PartyPubKeyHex, Proof, Signature,
};
use crate::PriKeyHex;

#[async_trait]
pub trait Adapter<B: Blk, F: FullBlk<B>, S: St>: 'static + Send + Sync {
    type CryptoImpl: Crypto;

    /// Get blocks exec result that exists already. Ensure height < latest_height.
    async fn get_block_exec_result(
        &self,
        ctx: Context,
        height: Height,
    ) -> Result<ExecResult<S>, Box<dyn Error + Send>>;

    #[allow(clippy::too_many_arguments)]
    async fn create_block(
        &self,
        ctx: Context,
        height: Height,
        exec_height: Height,
        pre_hash: Hash,
        pre_proof: Proof,
        block_states: Vec<BlockState<S>>,
        last_commit_exec_resp: S,
    ) -> Result<B, Box<dyn Error + Send>>;

    async fn check_block(
        &self,
        ctx: Context,
        block: B,
        block_states: Vec<BlockState<S>>,
        last_commit_exec_resp: S,
    ) -> Result<(), Box<dyn Error + Send>>;

    async fn fetch_full_block(&self, ctx: Context, block: B) -> Result<F, Box<dyn Error + Send>>;

    async fn save_full_block_with_proof(
        &self,
        ctx: Context,
        height: Height,
        full_block: F,
        proof: Proof,
        is_sync: bool,
    ) -> Result<(), Box<dyn Error + Send>>;

    async fn exec_full_block(
        &self,
        ctx: Context,
        height: Height,
        full_block: F,
        last_exec_resp: S,
        last_commit_exec_resp: S,
    ) -> Result<ExecResult<S>, Box<dyn Error + Send>>;

    async fn commit(&self, ctx: Context, commit_state: ExecResult<S>);

    async fn register_network(
        &self,
        ctx: Context,
        sender: UnboundedSender<(Context, OverlordMsg<B, F>)>,
    );

    async fn broadcast(
        &self,
        ctx: Context,
        msg: OverlordMsg<B, F>,
    ) -> Result<(), Box<dyn Error + Send>>;

    async fn transmit(
        &self,
        ctx: Context,
        to: Address,
        msg: OverlordMsg<B, F>,
    ) -> Result<(), Box<dyn Error + Send>>;

    /// should return empty vec if the required blocks are not exist
    async fn get_full_block_with_proofs(
        &self,
        ctx: Context,
        height_range: HeightRange,
    ) -> Result<Vec<FullBlockWithProof<B, F>>, Box<dyn Error + Send>>;

    async fn get_latest_height(&self, ctx: Context) -> Result<Height, Box<dyn Error + Send>>;

    async fn handle_error(&self, ctx: Context, err: OverlordError);
}

pub trait Blk: 'static + Clone + Debug + Display + Default + PartialEq + Eq + Send + Sync {
    /// should ensure the same serialization results in different environments
    fn fixed_encode(&self) -> Result<Bytes, Box<dyn Error + Send>>;

    fn fixed_decode(data: &Bytes) -> Result<Self, Box<dyn Error + Send>>;

    fn get_block_hash(&self) -> Result<Hash, Box<dyn Error + Send>>;

    fn get_pre_hash(&self) -> Hash;

    fn get_height(&self) -> Height;

    fn get_exec_height(&self) -> Height;

    fn get_proof(&self) -> Proof;
}

pub trait FullBlk<B: Blk>: 'static + Clone + Debug + Display + Default + Send + Sync {
    fn fixed_encode(&self) -> Result<Bytes, Box<dyn Error + Send>>;

    fn fixed_decode(data: &Bytes) -> Result<Self, Box<dyn Error + Send>>;

    fn get_block(&self) -> B;
}

pub trait St: 'static + Clone + Debug + Display + Default + Send + Sync {}

/// provide DefaultCrypto
pub trait Crypto: Send {
    fn hash(msg: &Bytes) -> Hash;

    fn sign_msg(pri_key: PriKeyHex, hash: &Hash) -> Result<Signature, Box<dyn Error + Send>>;

    fn verify_signature(
        pub_key: PubKeyHex,
        address: &Address,
        hash: &Hash,
        signature: &Signature,
    ) -> Result<(), Box<dyn Error + Send>>;

    fn party_sign_msg(pri_key: PriKeyHex, hash: &Hash) -> Result<Signature, Box<dyn Error + Send>>;

    fn party_verify_signature(
        common_ref: CommonHex,
        party_pub_key: PartyPubKeyHex,
        hash: &Hash,
        signature: &Signature,
    ) -> Result<(), Box<dyn Error + Send>>;

    fn aggregate(
        signatures: HashMap<&PartyPubKeyHex, &Signature>,
    ) -> Result<Signature, Box<dyn Error + Send>>;

    fn verify_aggregates(
        common_ref: CommonHex,
        hash: &Hash,
        party_pub_keys: &[PartyPubKeyHex],
        signature: &Signature,
    ) -> Result<(), Box<dyn Error + Send>>;
}
