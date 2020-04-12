pub mod error;
pub mod supports;
pub mod traits;
pub mod types;

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

pub use error::{ConsensusError, ConsensusResult};
pub use supports::{
    default_crypto::{gen_key_pairs, DefaultCrypto},
    default_wal::DefaultWal,
};
pub use traits::{Adapter, Blk, Crypto, Wal};
pub use types::{
    Address, BlockState, ConsensusConfig, ExecResult, Hash, Height, HeightRange, OverlordMsg,
    Proof, Round, Signature,
};

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedSender};

#[derive(Clone)]
pub struct OverlordServer<A, B: Blk, C, S, W> {
    inner:     UnboundedSender<(Context, OverlordMsg<B>)>,
    phantom_a: PhantomData<A>,
    phantom_c: PhantomData<C>,
    phantom_s: PhantomData<S>,
    phantom_w: PhantomData<W>,
}

impl<A, B, C, S, W> OverlordServer<A, B, C, S, W>
where
    A: Adapter<B, S>,
    B: Blk,
    C: Crypto,
    S: Clone + Debug + Default,
    W: Wal,
{
    // run overlord
    pub fn new(_address: Address, _adapter: Arc<A>, _crypto: Arc<C>, _wal: Arc<W>) -> Self {
        let (sender, _receiver) = unbounded();
        // Todo: run overlord

        OverlordServer {
            inner:     sender,
            phantom_a: PhantomData,
            phantom_c: PhantomData,
            phantom_s: PhantomData,
            phantom_w: PhantomData,
        }
    }
}
