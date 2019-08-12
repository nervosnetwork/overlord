//! Overlord Consensus Protocol

#![deny(missing_docs)]

use std::fmt::Debug;

use async_trait::async_trait;

use crate::types::{Address, AggregatedSignature, Commit, Hash, OutputMsg, Signature, Status};

///
pub(crate) mod smr;
///
pub mod state;
///
pub(crate) mod timer;
///
pub mod types;
///
pub(crate) mod utils;
///
pub(crate) mod wal;

///
#[async_trait]
pub trait Consensus<T: Codec>: Send + Sync {
    /// Consensus error
    type Error: ::std::error::Error;
    /// Get an epoch of an epoch_id and return the epoch with its hash.
    async fn get_epoch(&self, _ctx: Vec<u8>, epoch_id: u64) -> Result<(T, Hash), Self::Error>;
    /// Check the correctness of an epoch.
    async fn check_epoch(&self, _ctx: Vec<u8>, hash: Hash) -> Result<(), Self::Error>;
    /// TODO argc and return value
    async fn commit(
        &self,
        _ctx: Vec<u8>,
        epoch_id: u64,
        commit: Commit<T>,
    ) -> Result<Status, Self::Error>;
    /// Broadcast a message to other replicas.
    async fn broadcast_to_other(&self, _ctx: Vec<u8>, msg: OutputMsg<T>)
        -> Result<(), Self::Error>;
    /// Transmit a message to the Relayer.
    async fn transmit_to_relayer(
        &self,
        _ctx: Vec<u8>,
        addr: Address,
        msg: OutputMsg<T>,
    ) -> Result<(), Self::Error>;
}

///
#[async_trait]
pub trait Codec: Clone + Debug + Send + Sync {
    /// Codec error.
    type Error: ::std::error::Error;
    /// Asynchronous serialize function.
    async fn serialize(&self) -> Result<Vec<u8>, Self::Error>;
    /// Asynchronous deserialize function.
    async fn deserialize(data: Vec<u8>) -> Result<Self, Self::Error>;
}

///
pub trait Crypto {
    /// Crypto error.
    type Error: ::std::error::Error;
    /// Hash a message.
    fn hash(&self, msg: &[u8]) -> Hash;
    /// Sign to the given hash by private key.
    fn sign(&self, hash: Hash) -> Result<Signature, Self::Error>;
    /// Aggregate signatures into an aggregated signature.
    fn aggregate_signatures(&self, signatures: Vec<Signature>) -> Result<Signature, Self::Error>;
    /// Verify a signature.
    fn verify_signature(&self, signature: Signature, hash: Hash) -> Result<Address, Self::Error>;
    /// Verify an aggregated signature.
    fn verify_aggregated_signature(
        &self,
        aggregate_signature: AggregatedSignature,
    ) -> Result<(), Self::Error>;
}
