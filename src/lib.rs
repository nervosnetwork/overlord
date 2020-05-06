//! Overlord Consensus Protocol is a Byzantine fault tolerance (BFT) consensus algorithm aiming to
//! support thousands of transactions per second under hundreds of consensus nodes, with transaction
//! delays of no more than a few seconds. Simply put, it is a high-performance consensus algorithm
//! able to meets most of the real business needs.

#![deny(missing_docs)]
#![recursion_limit = "256"]
#![feature(test)]

/// A module that impl rlp encodable and decodable trait for types that need to save wal.
mod codec;
/// Overlord error module.
pub mod error;
/// Create and run the overlord consensus process.
pub mod overlord;
/// serialize Bytes in hex format
pub mod serde_hex;
/// serialize Vec<Bytes> in hex format
mod serde_multi_hex;
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
pub use self::utils::auth_manage::extract_voters;
pub use creep::Context;
pub use wal::WalInfo;

use std::error::Error;
use std::fmt::Debug;

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::error::ConsensusError;
use crate::types::{Address, Commit, Hash, Node, OverlordMsg, Signature, Status};

/// Overlord consensus result.
pub type ConsensusResult<T> = ::std::result::Result<T, ConsensusError>;

const INIT_HEIGHT: u64 = 0;
const INIT_ROUND: u64 = 0;

/// Trait for some functions that consensus needs.
#[async_trait]
pub trait Consensus<T: Codec>: Send + Sync {
    /// Get a block of the given height and return the block with its hash.
    async fn get_block(
        &self,
        ctx: Context,
        height: u64,
    ) -> Result<(T, Hash), Box<dyn Error + Send>>;

    /// Check the correctness of a block. If is passed, return the integrated transcations to do
    /// data persistence.
    async fn check_block(
        &self,
        ctx: Context,
        height: u64,
        hash: Hash,
        block: T,
    ) -> Result<(), Box<dyn Error + Send>>;

    /// Commit a given height to execute and return the rich status.
    async fn commit(
        &self,
        ctx: Context,
        height: u64,
        commit: Commit<T>,
    ) -> Result<Status, Box<dyn Error + Send>>;

    /// Get an authority list of the given height.
    async fn get_authority_list(
        &self,
        ctx: Context,
        height: u64,
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

    /// Report the overlord error with the corresponding context.
    fn report_error(&self, ctx: Context, error: ConsensusError);
}

/// Trait for doing serialize and deserialize.
pub trait Codec: Clone + Debug + Send + PartialEq + Eq {
    /// Serialize self into bytes.
    fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>>;

    /// Deserialize date into self.
    fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>>;
}

/// Trait for save and load wal information.
#[async_trait]
pub trait Wal {
    /// Save wal information.
    async fn save(&self, info: Bytes) -> Result<(), Box<dyn Error + Send>>;

    /// Load wal information.
    async fn load(&self) -> Result<Option<Bytes>, Box<dyn Error + Send>>;
}

/// Trait for some crypto methods.
pub trait Crypto: Send {
    /// Hash a message bytes.
    fn hash(&self, msg: Bytes) -> Hash;

    /// Sign to the given hash by private key and return the signature if success.
    fn sign(&self, hash: Hash) -> Result<Signature, Box<dyn Error + Send>>;

    /// Aggregate the given signatures into an aggregated signature according to the given bitmap.
    fn aggregate_signatures(
        &self,
        signatures: Vec<Signature>,
        voters: Vec<Address>,
    ) -> Result<Signature, Box<dyn Error + Send>>;

    /// Verify a signature and return the recovered address.
    fn verify_signature(
        &self,
        signature: Signature,
        hash: Hash,
        voter: Address,
    ) -> Result<(), Box<dyn Error + Send>>;

    /// Verify an aggregated signature.
    fn verify_aggregated_signature(
        &self,
        aggregate_signature: Signature,
        msg_hash: Hash,
        voters: Vec<Address>,
    ) -> Result<(), Box<dyn Error + Send>>;
}

/// The setting of the timeout interval of each step.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DurationConfig {
    /// The proportion of propose timeout to the height interval.
    pub propose_ratio: u64,
    /// The proportion of prevote timeout to the height interval.
    pub prevote_ratio: u64,
    /// The proportion of precommit timeout to the height interval.
    pub precommit_ratio: u64,
    /// The proportion of retry choke message timeout to the height interval.
    pub brake_ratio: u64,
}

impl Default for DurationConfig {
    fn default() -> Self {
        DurationConfig {
            propose_ratio:   0u64,
            prevote_ratio:   0u64,
            precommit_ratio: 0u64,
            brake_ratio:     0u64,
        }
    }
}

impl DurationConfig {
    /// Create a consensus timeout configuration.
    pub fn new(
        propose_ratio: u64,
        prevote_ratio: u64,
        precommit_ratio: u64,
        brake_ratio: u64,
    ) -> Self {
        DurationConfig {
            propose_ratio,
            prevote_ratio,
            precommit_ratio,
            brake_ratio,
        }
    }

    pub(crate) fn get_propose_config(&self) -> (u64, u64) {
        (self.propose_ratio, 10u64)
    }

    pub(crate) fn get_prevote_config(&self) -> (u64, u64) {
        (self.prevote_ratio, 10u64)
    }

    pub(crate) fn get_precommit_config(&self) -> (u64, u64) {
        (self.precommit_ratio, 10u64)
    }

    pub(crate) fn get_brake_config(&self) -> (u64, u64) {
        (self.brake_ratio, 10u64)
    }
}

#[cfg(test)]
mod test {
    use super::DurationConfig;

    #[test]
    fn test_duration_config() {
        let config = DurationConfig::new(1, 2, 3, 4);
        assert_eq!(config.get_propose_config(), (1, 10));
        assert_eq!(config.get_prevote_config(), (2, 10));
        assert_eq!(config.get_precommit_config(), (3, 10));
        assert_eq!(config.get_brake_config(), (4, 10));
    }
}
