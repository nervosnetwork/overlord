//! Overlord Consensus Protocol

#![deny(missing_docs)]

/// A module that impl rlp encodable and decodable trait
/// for types that need to save wal.
mod codec;
/// Consensus error module.
mod error;
///
mod smr;
///
pub mod state;
///
mod timer;
///
pub mod types;
///
mod utils;
///
mod wal;

use std::error::Error;

use async_trait::async_trait;
use bytes::Bytes;

use crate::error::ConsensusError;
use crate::types::{
    Address, AggregatedSignature, Commit, Hash, Node, OverlordMsg, Signature, Status,
};

/// Overlord consensus result.
pub type ConsensusResult<T> = ::std::result::Result<T, ConsensusError>;

const INIT_EPOCH_ID: u64 = 0;
const INIT_ROUND: u64 = 0;

/// **TODO: context libiary**
#[async_trait]
pub trait Consensus<T: Codec>: Send + Sync {
    /// Get an epoch of an epoch_id and return the epoch with its hash.
    async fn get_epoch(
        &self,
        _ctx: Vec<u8>,
        epoch_id: u64,
    ) -> Result<(T, Hash), Box<dyn Error + Send>>;
    /// Check the correctness of an epoch.
    async fn check_epoch(
        &self,
        _ctx: Vec<u8>,
        epoch_id: u64,
        hash: Hash,
    ) -> Result<T, Box<dyn Error + Send>>;
    /// Commit an epoch to execute and return the rich status.
    async fn commit(
        &self,
        _ctx: Vec<u8>,
        epoch_id: u64,
        commit: Commit<T>,
    ) -> Result<Status, Box<dyn Error + Send>>;
    /// Get an authority list of the given epoch.
    async fn get_authority_list(
        &self,
        _ctx: Vec<u8>,
        epoch_id: u64,
    ) -> Result<Vec<Node>, Box<dyn Error + Send>>;
    /// Broadcast a message to other replicas.
    async fn broadcast_to_other(
        &self,
        _ctx: Vec<u8>,
        msg: OverlordMsg<T>,
    ) -> Result<(), Box<dyn Error + Send>>;
    /// Transmit a message to the Relayer.
    async fn transmit_to_relayer(
        &self,
        _ctx: Vec<u8>,
        addr: Address,
        msg: OverlordMsg<T>,
    ) -> Result<(), Box<dyn Error + Send>>;
}

///
pub trait Codec: Clone + Send {
    /// Serialize self into bytes.
    fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>>;
    /// Deserialize date into self.
    fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>>;
}

///
pub trait Crypto: Clone + Send {
    /// Hash a message.
    fn hash(&self, msg: Bytes) -> Hash;
    /// Sign to the given hash by private key.
    fn sign(&self, hash: Hash) -> Result<Signature, Box<dyn Error + Send>>;
    /// Aggregate signatures into an aggregated signature.
    fn aggregate_signatures(
        &self,
        signatures: Vec<Signature>,
    ) -> Result<Signature, Box<dyn Error + Send>>;
    /// Verify a signature.
    fn verify_signature(
        &self,
        signature: Signature,
        hash: Hash,
    ) -> Result<Address, Box<dyn Error + Send>>;
    /// Verify an aggregated signature.
    fn verify_aggregated_signature(
        &self,
        aggregate_signature: AggregatedSignature,
    ) -> Result<(), Box<dyn Error + Send>>;
}
