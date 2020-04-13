#![allow(dead_code)]

pub mod crypto;
pub mod error;
pub mod traits;
pub mod types;

mod collections;
mod wal;

pub use crypto::{gen_key_pairs, AddressHex, BlsPubKeyHex, DefaultCrypto, PriKeyHex};
pub use error::{ConsensusError, ConsensusResult};
pub use traits::{Adapter, Blk, Crypto};
pub use types::{
    Address, BlockState, ConsensusConfig, DurationConfig, ExecResult, Hash, Height, HeightRange,
    Node, OverlordMsg, Proof, Round, Signature,
};

use std::fmt::Debug;
use std::marker::PhantomData;

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedReceiver};

use crate::wal::Wal;

pub struct OverlordServer<A, B: Blk, C, S> {
    adapter: A,
    crypto:  C,
    network: UnboundedReceiver<(Context, OverlordMsg<B>)>,
    wal:     Wal,

    address: Address,

    phantom_data: PhantomData<S>,
}

impl<A, B, C, S> OverlordServer<A, B, C, S>
where
    A: Adapter<B, S>,
    B: Blk,
    C: Crypto,
    S: Clone + Debug + Default,
{
    pub fn new(address: Address, adapter: A, crypto: C, wal_path: &str) -> Self {
        let (sender, receiver) = unbounded();
        adapter.register_network(Context::default(), sender);

        OverlordServer {
            adapter,
            crypto,
            network: receiver,
            wal: Wal::new(wal_path),
            address,
            phantom_data: PhantomData,
        }
    }

    // Todo: run overlord
    pub fn run(&self) {}
}
