#![allow(unused_imports)]

pub use creep::Context;

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;

use async_trait::async_trait;
use bytes::Bytes;
use futures::channel::mpsc::UnboundedSender;

use crate::crypto::PubKeyHex;
use crate::error::OverlordError;
use crate::types::{
    Address, Aggregates, BlockState, CommonHex, ExecResult, Hash, Height, HeightRange, OverlordMsg,
    Proof, Signature,
};
use crate::PriKeyHex;

#[async_trait]
pub trait Adapter<B: Blk, S: St>: 'static + Send + Sync + Unpin {
    type CryptoImpl: Crypto;

    async fn create_block(
        &self,
        ctx: Context,
        height: Height,
        exec_height: Height,
        pre_hash: Hash,
        pre_proof: Proof,
        block_states: Vec<BlockState<S>>,
    ) -> Result<B, Box<dyn Error + Send>>;

    async fn check_block_states(
        &self,
        ctx: Context,
        block: &B,
        block_states: &[BlockState<S>],
    ) -> Result<(), Box<dyn Error + Send>>;

    async fn fetch_full_block(
        &self,
        ctx: Context,
        block: &B,
    ) -> Result<Bytes, Box<dyn Error + Send>>;

    async fn save_and_exec_block_with_proof(
        &self,
        ctx: Context,
        height: Height,
        full_block: Bytes,
        proof: Proof,
    ) -> Result<ExecResult<S>, Box<dyn Error + Send>>;

    async fn register_network(
        &self,
        _ctx: Context,
        sender: UnboundedSender<(Context, OverlordMsg<B>)>,
    );

    async fn broadcast(
        &self,
        ctx: Context,
        msg: OverlordMsg<B>,
    ) -> Result<(), Box<dyn Error + Send>>;

    async fn transmit(
        &self,
        ctx: Context,
        to: Address,
        msg: OverlordMsg<B>,
    ) -> Result<(), Box<dyn Error + Send>>;

    /// should return empty vec if the required blocks are not exist
    async fn get_block_with_proofs(
        &self,
        ctx: Context,
        height_range: HeightRange,
    ) -> Result<Vec<(B, Proof)>, Box<dyn Error + Send>>;

    async fn get_latest_height(&self, ctx: Context) -> Result<Height, Box<dyn Error + Send>>;

    async fn handle_error(&self, ctx: Context, err: OverlordError);
}

/// should ensure the same serialization results in different environments
pub trait Blk: 'static + Clone + Debug + Default + PartialEq + Eq + Send + Unpin {
    fn fixed_encode(&self) -> Result<Bytes, Box<dyn Error + Send>>;

    fn fixed_decode(data: &Bytes) -> Result<Self, Box<dyn Error + Send>>;

    fn get_block_hash(&self) -> Hash;

    fn get_pre_hash(&self) -> Hash;

    fn get_height(&self) -> Height;

    fn get_exec_height(&self) -> Height;

    fn get_proof(&self) -> Proof;
}

pub trait St: 'static + Clone + Debug + Default + Send + Unpin {}

/// provide DefaultCrypto
pub trait Crypto: Send {
    fn hash(msg: &Bytes) -> Hash;

    fn sign(pri_key: PriKeyHex, hash: &Hash) -> Result<Signature, Box<dyn Error + Send>>;

    fn verify_signature(
        common_ref: CommonHex,
        pub_key: PubKeyHex,
        hash: &Hash,
        signature: &Signature,
    ) -> Result<(), Box<dyn Error + Send>>;

    fn aggregate(
        signatures: HashMap<&PubKeyHex, &Signature>,
    ) -> Result<Signature, Box<dyn Error + Send>>;

    fn verify_aggregates(
        common_ref: CommonHex,
        hash: &Hash,
        pub_keys: &[PubKeyHex],
        signature: &Signature,
    ) -> Result<(), Box<dyn Error + Send>>;
}
