pub mod crypto;
pub mod error;
pub mod traits;
pub mod types;

mod wal;

pub use crypto::{gen_key_pairs, DefaultCrypto};
pub use error::{ConsensusError, ConsensusResult};
pub use traits::{Adapter, Blk, Crypto};
pub use types::{
    Address, BlockState, ConsensusConfig, ExecResult, Hash, Height, HeightRange, OverlordMsg,
    Proof, Round, Signature,
};

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};

pub struct OverlordServer<A, B: Blk, C, S> {
    adapter: Arc<A>,
    crypto:  C,
    network: UnboundedReceiver<(Context, OverlordMsg<B>)>,

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
    pub fn new(address: Address, adapter: &Arc<A>, crypto: C) -> Self {
        let (sender, receiver) = unbounded();
        adapter.register_network(Context::default(), sender);

        OverlordServer {
            adapter: Arc::<A>::clone(adapter),
            crypto,
            network: receiver,
            address,
            phantom_data: PhantomData,
        }
    }

    // Todo: run overlord
    pub fn run() {}
}
