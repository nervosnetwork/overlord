//! Overlord Consensus Protocol

#![deny(missing_docs)]
// Remove this clippy bug with async await is resolved.
// ISSUE: https://github.com/rust-lang/rust-clippy/issues/3988
#![allow(clippy::needless_lifetimes)]

use std::error::Error;

use async_trait::async_trait;
use bytes::Bytes;

use crate::types::{Address, AggregatedSignature, Commit, Hash, OutputMsg, Signature, Status};

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

///
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
    ) -> Result<(), Box<dyn Error + Send>>;
    /// TODO argc and return value
    async fn commit(
        &self,
        _ctx: Vec<u8>,
        epoch_id: u64,
        commit: Commit<T>,
    ) -> Result<Status, Box<dyn Error + Send>>;
    /// Broadcast a message to other replicas.
    async fn broadcast_to_other(
        &self,
        _ctx: Vec<u8>,
        msg: OutputMsg<T>,
    ) -> Result<(), Box<dyn Error + Send>>;
    /// Transmit a message to the Relayer.
    async fn transmit_to_relayer(
        &self,
        _ctx: Vec<u8>,
        addr: Address,
        msg: OutputMsg<T>,
    ) -> Result<(), Box<dyn Error + Send>>;
}

///
#[async_trait]
pub trait Codec: Clone + Send {
    /// Asynchronous serialize function.
    async fn serialize(&self) -> Result<Bytes, Box<dyn Error + Send>>;
    /// Asynchronous deserialize function.
    async fn deserialize(data: Bytes) -> Result<Self, Box<dyn Error + Send>>;
}

///
pub trait Crypto {
    /// Hash a message.
    fn hash(&self, msg: &[u8]) -> Hash;
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
