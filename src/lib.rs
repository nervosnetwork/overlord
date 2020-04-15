#![allow(dead_code)]

pub mod crypto;
pub mod error;
pub mod traits;
pub mod types;

mod auth;
mod cabinet;
mod codec;
mod event;
mod smr;
mod state;
mod timer;
mod wal;

pub use crypto::{gen_key_pairs, AddressHex, BlsPubKeyHex, DefaultCrypto, PriKeyHex};
pub use error::{ConsensusError, ConsensusResult};
pub use traits::{Adapter, Blk, Crypto, St};
pub use types::{
    Address, AuthConfig, BlockState, CommonHex, ExecResult, Hash, Height, HeightRange, Node,
    OverlordConfig, OverlordMsg, Proof, Round, Signature, TimeConfig,
};

use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::unbounded;

use crate::auth::AuthFixedConfig;
use crate::smr::StateMachine;
use crate::timer::Timer;
use crate::wal::Wal;

const INIT_HEIGHT: Height = 0;
const INIT_ROUND: Round = 0;

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
        let (net_sender, net_receiver) = unbounded();
        let (smr_sender, smr_receiver) = unbounded();
        let (timer_sender, timer_receiver) = unbounded();
        adapter.register_network(Context::default(), net_sender);
        let auth_fixed_config = AuthFixedConfig::new(common_ref, pri_key, address);
        let state_machine = StateMachine::new(
            auth_fixed_config,
            adapter,
            net_receiver,
            timer_receiver,
            smr_sender,
            wal_path,
        );

        state_machine.run();

        let timer = Timer::new(smr_receiver, timer_sender);
        timer.run()
    }
}
