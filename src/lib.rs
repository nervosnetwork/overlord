#![allow(dead_code)]

pub mod crypto;
pub mod error;
pub mod traits;
pub mod types;

mod auth_manage;
mod cabinet;
mod codec;
mod state_machine;
mod timer;
mod wal;

pub use crypto::{gen_key_pairs, AddressHex, BlsPubKeyHex, DefaultCrypto, PriKeyHex};
pub use error::{ConsensusError, ConsensusResult};
pub use traits::{Adapter, Blk, Crypto, St};
pub use types::{
    Address, AuthConfig, BlockState, CommonHex, ExecResult, Hash, Height, HeightRange, Node,
    OverlordConfig, OverlordMsg, Proof, Round, Signature, TimeConfig,
};

use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::unbounded;

use crate::state_machine::StateMachine;
use crate::wal::Wal;
use serde::export::PhantomData;

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
    pub fn run(
        common_ref: CommonHex,
        pri_key: PriKeyHex,
        address: Address,
        adapter: &Arc<A>,
        wal_path: &str,
    ) {
        let (sender, receiver) = unbounded();
        adapter.register_network(Context::default(), sender);
        let state_machine =
            StateMachine::new(common_ref, pri_key, address, adapter, receiver, wal_path);

        state_machine.run();
    }
}
