pub mod error;
pub mod types;

pub use creep::Context;

use std::error::Error;
use std::fmt::Debug;

use async_trait::async_trait;
use bytes::Bytes;
use futures::channel::mpsc::UnboundedReceiver;

use crate::error::ConsensusError;
use crate::types::{Address, BlockState, ExecResult, Hash, Height, OverlordMsg, Proof, Signature};

const INIT_HEIGHT: u64 = 0;
const INIT_ROUND: u64 = 0;

#[async_trait]
pub trait Consensus<B: Blk, S: Clone + Debug>: Send + Sync {
    async fn create_block(
        &self,
        ctx: Context,
        height: Height,
        exec_height: Height,
        pre_hash: Hash,
        pre_proof: Proof,
        block_states: Vec<BlockState<S>>,
    ) -> Result<(B, Hash), Box<dyn Error + Send>>;

    async fn check_block(
        &self,
        ctx: Context,
        height: Height,
        block_hash: Hash,
        block: B,
    ) -> Result<(), Box<dyn Error + Send>>;

    async fn exec_block(
        &self,
        ctx: Context,
        height: Height,
        block: B,
    ) -> Result<ExecResult<S>, Box<dyn Error + Send>>;

    async fn broadcast(
        &self,
        ctx: Context,
        msg: OverlordMsg<B>,
    ) -> Result<(), Box<dyn Error + Send>>;

    async fn transmit(
        &self,
        ctx: Context,
        addr: Address,
        msg: OverlordMsg<B>,
    ) -> Result<(), Box<dyn Error + Send>>;

    async fn receiver(&self, ctx: Context) -> UnboundedReceiver<OverlordMsg<B>>;

    async fn get_block(
        &self,
        ctx: Context,
        height: Height,
    ) -> Result<(B, Proof), Box<dyn Error + Send>>;

    async fn handle_error(&self, ctx: Context, err: ConsensusError);
}

/// should ensure the same serialization results in different environments
pub trait Blk: Clone + Debug + Send + PartialEq + Eq {
    fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>>;

    fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>>;

    fn get_pre_hash(&self) -> Result<Hash, Box<dyn Error + Send>>;

    fn get_exec_height(&self) -> Result<Height, Box<dyn Error + Send>>;

    fn get_proof(&self) -> Result<Proof, Box<dyn Error + Send>>;
}

/// provide DefaultWal
#[async_trait]
pub trait Wal {
    async fn save(&self, info: Bytes) -> Result<(), Box<dyn Error + Send>>;

    async fn load(&self) -> Result<Option<Bytes>, Box<dyn Error + Send>>;
}

/// provide DefaultCrypto
pub trait Crypto: Send {
    fn hash(&self, msg: Bytes) -> Hash;

    fn sign(&self, hash: Hash) -> Result<Signature, Box<dyn Error + Send>>;

    fn aggregate_signatures(
        &self,
        signatures: Vec<Signature>,
        voters: Vec<Address>,
    ) -> Result<Signature, Box<dyn Error + Send>>;

    fn verify_signature(
        &self,
        signature: Signature,
        hash: Hash,
        voter: Address,
    ) -> Result<(), Box<dyn Error + Send>>;

    fn verify_aggregated_signature(
        &self,
        aggregate_signature: Signature,
        msg_hash: Hash,
        voters: Vec<Address>,
    ) -> Result<(), Box<dyn Error + Send>>;
}
