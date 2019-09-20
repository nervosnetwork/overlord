//! Overlord Consensus Protocol

#![deny(missing_docs)]
#![feature(test)]

/// A module that impl rlp encodable and decodable trait for types that need to save wal.
mod codec;
/// Overlord error module.
pub mod error;
/// Create and run the overlord consensus process.
pub mod overlord;
/// State machine replicas module to do state changes.
mod smr;
/// The state module to storage proposals and votes.
mod state;
/// The timer module to ensure the protocol liveness.
mod timer;
/// Message types using in the overlord consensus protocol.
pub mod types;
/// Some utility functions.
mod utils;
/// Write ahead log module.
mod wal;

pub use self::overlord::Overlord;
pub use self::overlord::OverlordHandler;
pub use creep::Context;

use std::error::Error;
use std::fmt::Debug;

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

/// A necessary trait to keep
#[async_trait]
pub trait Consensus<T: Codec, S: Codec>: Send + Sync {
    /// Get an epoch of the given epoch ID and return the epoch with its hash.
    async fn get_epoch(
        &self,
        ctx: Context,
        epoch_id: u64,
    ) -> Result<(T, Hash), Box<dyn Error + Send>>;

    /// Check the correctness of an epoch. If is passed, return the integrated transcations to do
    /// data persistence.
    async fn check_epoch(
        &self,
        ctx: Context,
        epoch_id: u64,
        hash: Hash,
    ) -> Result<S, Box<dyn Error + Send>>;

    /// Commit a given epoch to execute and return the rich status.
    async fn commit(
        &self,
        ctx: Context,
        epoch_id: u64,
        commit: Commit<T>,
    ) -> Result<Status, Box<dyn Error + Send>>;

    /// Get an authority list of the given epoch ID.
    async fn get_authority_list(
        &self,
        ctx: Context,
        epoch_id: u64,
    ) -> Result<Vec<Node>, Box<dyn Error + Send>>;

    /// Broadcast a message to other replicas.
    async fn broadcast_to_other(
        &self,
        ctx: Context,
        msg: OverlordMsg<T>,
    ) -> Result<(), Box<dyn Error + Send>>;

    /// Transmit a message to the Relayer, the third argument is the relayer's address.
    async fn transmit_to_relayer(
        &self,
        ctx: Context,
        addr: Address,
        msg: OverlordMsg<T>,
    ) -> Result<(), Box<dyn Error + Send>>;
}

/// Trait for doing serialize and deserialize.
pub trait Codec: Clone + Debug + Send {
    /// Serialize self into bytes.
    fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>>;

    /// Deserialize date into self.
    fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>>;
}

/// Trait for some crypto methods.
pub trait Crypto: Clone + Send {
    /// Hash a message bytes.
    fn hash(&self, msg: Bytes) -> Hash;

    /// Sign to the given hash by private key and return the signature if success.
    fn sign(&self, hash: Hash) -> Result<Signature, Box<dyn Error + Send>>;

    /// Aggregate the given signatures into an aggregated signature according to the given bitmap.
    fn aggregate_signatures(
        &self,
        signatures: Vec<Signature>,
    ) -> Result<Signature, Box<dyn Error + Send>>;

    /// Verify a signature and return the recovered address.
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
