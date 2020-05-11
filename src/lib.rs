#![recursion_limit = "512"]

#[macro_use]
mod r#macro;

pub mod crypto;
pub mod error;
pub mod traits;
pub mod types;

mod codec;
mod smr;
mod state;
mod utils;

pub use crypto::{gen_key_pairs, AddressHex, BlsPubKeyHex, DefaultCrypto, PriKeyHex};
pub use error::{OverlordError, OverlordResult};
pub use traits::{Adapter, Blk, Crypto, FullBlk, St};
pub use types::{
    Address, AuthConfig, BlockState, CommonHex, CryptoConfig, ExecResult, Hash, Height,
    HeightRange, Node, OverlordConfig, OverlordMsg, PartyPubKeyHex, Proof, Round, Signature,
    TimeConfig, TinyHex,
};

use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::unbounded;

use crate::smr::SMR;
use crate::utils::exec::Exec;
use crate::utils::wal::Wal;

pub const INIT_HEIGHT: Height = 0;
pub const INIT_ROUND: Round = 0;

pub struct OverlordServer<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St> {
    phantom_a: PhantomData<A>,
    phantom_b: PhantomData<B>,
    phantom_f: PhantomData<F>,
    phantom_s: PhantomData<S>,
}

impl<A, B, F, S> OverlordServer<A, B, F, S>
where
    A: Adapter<B, F, S>,
    B: Blk,
    F: FullBlk<B>,
    S: St,
{
    pub async fn run(ctx: Context, crypto_config: CryptoConfig, adapter: &Arc<A>, wal_path: &str) {
        let (net_sender, net_receiver) = unbounded();
        let (smr_sender, smr_receiver) = unbounded();
        let (exec_sender, exec_receiver) = unbounded();

        adapter
            .register_network(ctx.clone(), net_sender.clone())
            .await;

        let exec = Exec::new(adapter, smr_receiver, exec_sender);
        let smr = SMR::new(
            ctx.clone(),
            crypto_config,
            adapter,
            net_receiver,
            net_sender,
            exec_receiver,
            smr_sender,
            wal_path,
        )
        .await;

        exec.run();
        smr.run(ctx).await;
    }
}
