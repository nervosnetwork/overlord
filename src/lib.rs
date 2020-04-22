#![recursion_limit = "512"]

#[macro_use]
mod r#macro;

pub mod crypto;
pub mod error;
pub mod traits;
pub mod types;

mod auth;
mod cabinet;
mod codec;
mod exec;
mod smr;
mod state;
mod sync;
mod timeout;
mod wal;

pub use crypto::{gen_key_pairs, AddressHex, BlsPubKeyHex, DefaultCrypto, PriKeyHex};
pub use error::{OverlordError, OverlordResult};
pub use traits::{Adapter, Blk, Crypto, St};
pub use types::{
    Address, AuthConfig, BlockState, CommonHex, ExecResult, Hash, Height, HeightRange, Node,
    OverlordConfig, OverlordMsg, Proof, Round, Signature, TimeConfig, TinyHex,
};

use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::unbounded;

use crate::auth::AuthFixedConfig;
use crate::exec::Exec;
use crate::smr::SMR;
use crate::types::{PartyPubKeyHex, PubKeyHex};
use crate::wal::Wal;

pub const INIT_HEIGHT: Height = 0;
pub const INIT_ROUND: Round = 0;

pub struct OverlordServer<A: Adapter<B, S>, B: Blk, S: St> {
    phantom_a: PhantomData<A>,
    phantom_b: PhantomData<B>,
    phantom_s: PhantomData<S>,
}

impl<A, B, S> OverlordServer<A, B, S>
where
    A: Adapter<B, S>,
    B: Blk,
    S: St,
{
    pub async fn run(
        common_ref: CommonHex,
        pri_key: PriKeyHex,
        pub_key: PubKeyHex,
        party_pub_key: PartyPubKeyHex,
        address: Address,
        adapter: &Arc<A>,
        wal_path: &str,
    ) {
        let (net_sender, net_receiver) = unbounded();
        let (smr_sender, smr_receiver) = unbounded();
        let (exec_sender, exec_receiver) = unbounded();

        adapter
            .register_network(Context::default(), net_sender.clone())
            .await;
        let auth_fixed_config =
            AuthFixedConfig::new(common_ref, pri_key, pub_key, party_pub_key, address);

        let exec = Exec::new(adapter, smr_receiver, exec_sender);
        let smr = SMR::new(
            auth_fixed_config,
            adapter,
            net_receiver,
            net_sender,
            exec_receiver,
            smr_sender,
            wal_path,
        )
        .await;

        exec.run();
        smr.run().await;
    }
}
