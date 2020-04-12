pub mod crypto;
pub mod error;
pub mod traits;
pub mod types;

mod wal;

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

pub use crypto::{gen_key_pairs, DefaultCrypto};
pub use error::{ConsensusError, ConsensusResult};
pub use traits::{Adapter, Blk, Crypto};
pub use types::{
    Address, BlockState, ConsensusConfig, ExecResult, Hash, Height, HeightRange, OverlordMsg,
    Proof, Round, Signature,
};

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedSender};

#[derive(Clone)]
pub struct OverlordServer<A, B: Blk, C, S> {
    inner:     UnboundedSender<(Context, OverlordMsg<B>)>,
    phantom_a: PhantomData<A>,
    phantom_c: PhantomData<C>,
    phantom_s: PhantomData<S>,
}

impl<A, B, C, S> OverlordServer<A, B, C, S>
where
    A: Adapter<B, S>,
    B: Blk,
    C: Crypto,
    S: Clone + Debug + Default,
{
    // run overlord
    pub fn new(_address: Address, _adapter: Arc<A>, _crypto: Arc<C>) -> Self {
        let (sender, _receiver) = unbounded();
        // Todo: run overlord

        OverlordServer {
            inner:     sender,
            phantom_a: PhantomData,
            phantom_c: PhantomData,
            phantom_s: PhantomData,
        }
    }
}
