#![allow(dead_code)]

pub mod crypto;
pub mod error;
pub mod proof;
pub mod traits;
pub mod types;

mod cabinet;
mod wal;

pub use crypto::{gen_key_pairs, AddressHex, BlsPubKeyHex, DefaultCrypto, PriKeyHex};
pub use error::{ConsensusError, ConsensusResult};
pub use traits::{Adapter, Blk, Crypto};
pub use types::{
    Address, BlockState, CommonHex, DurationConfig, ExecResult, Hash, Height, HeightRange, Node,
    OverlordConfig, OverlordMsg, Proof, Round, Signature,
};

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedReceiver};

use crate::wal::Wal;

pub struct OverlordServer<A, B: Blk, S> {
    adapter: Arc<A>,
    network: UnboundedReceiver<(Context, OverlordMsg<B>)>,
    wal:     Wal,

    address: Address,

    phantom_s: PhantomData<S>,
}

impl<A, B, S> OverlordServer<A, B, S>
where
    A: Adapter<B, S>,
    B: Blk,
    S: Clone + Debug + Default,
{
    pub fn new(my_address: Address, adapter: &Arc<A>, wal_path: &str) -> Self {
        let (sender, receiver) = unbounded();
        adapter.register_network(Context::default(), sender);

        OverlordServer {
            adapter:   Arc::<A>::clone(adapter),
            network:   receiver,
            wal:       Wal::new(wal_path),
            address:   my_address,
            phantom_s: PhantomData,
        }
    }

    // Todo: run overlord
    pub fn run(&self) {}
}
