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
pub use types::{Address, Hash, Height, OverlordMsg, Proof, Round, Signature};

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedSender};

pub struct Overlord<A, B: Blk, C, S, W> {
    inner:     UnboundedSender<(Context, OverlordMsg<B>)>,
    phantom_a: PhantomData<A>,
    phantom_c: PhantomData<C>,
    phantom_s: PhantomData<S>,
    phantom_w: PhantomData<W>,
}

impl<A, B, C, S, W> Overlord<A, B, C, S, W>
where
    A: Adapter<B, S>,
    B: Blk,
    C: Crypto,
    S: Clone + Debug,
    W: Wal,
{
    // run overlord
    pub fn new(_address: Address, _adapter: Arc<A>, _crypto: Arc<C>, _wal: Arc<W>) -> Self {
        let (sender, _receiver) = unbounded();
        // Todo: run overlord

        Overlord {
            inner:     sender,
            phantom_a: PhantomData,
            phantom_c: PhantomData,
            phantom_s: PhantomData,
            phantom_w: PhantomData,
        }
    }

    pub fn send_msg(&self, ctx: Context, msg: OverlordMsg<B>) -> ConsensusResult<()> {
        self.inner
            .unbounded_send((ctx, msg))
            .map_err(|e| ConsensusError::Other(format!("Send message error {:?}", e)))
    }
}
