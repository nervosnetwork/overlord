#![allow(unused_imports)]

use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::auth::{AuthFixedConfig, AuthManage};
use crate::cabinet::Cabinet;
use crate::event::{SMREvent, TimerEvent};
use crate::state::Stage;
use crate::types::{Proposal, UpdateFrom};
use crate::{Adapter, Address, Blk, CommonHex, OverlordMsg, PriKeyHex, Round, St, Wal};

pub struct StateMachine<A: Adapter<B, S>, B: Blk, S: St> {
    stage: Stage,

    adapter: Arc<A>,
    wal:     Wal,
    cabinet: Cabinet<B>,
    auth:    AuthManage<A, B, S>,

    from_net:   UnboundedReceiver<(Context, OverlordMsg<B>)>,
    from_timer: UnboundedReceiver<TimerEvent>,
    to_timer:   UnboundedSender<SMREvent>,

    phantom_s: PhantomData<S>,
}

impl<A, B, S> StateMachine<A, B, S>
where
    A: Adapter<B, S>,
    B: Blk,
    S: St,
{
    pub fn new(
        auth_fixed_config: AuthFixedConfig,
        adapter: &Arc<A>,
        net_receiver: UnboundedReceiver<(Context, OverlordMsg<B>)>,
        timer_receiver: UnboundedReceiver<TimerEvent>,
        smr_sender: UnboundedSender<SMREvent>,
        wal_path: &str,
    ) -> Self {
        StateMachine {
            stage:      Stage::default(),
            adapter:    Arc::<A>::clone(adapter),
            wal:        Wal::new(wal_path),
            cabinet:    Cabinet::default(),
            auth:       AuthManage::new(auth_fixed_config),
            from_net:   net_receiver,
            from_timer: timer_receiver,
            to_timer:   smr_sender,
            phantom_s:  PhantomData,
        }
    }

    pub fn run(&self) {
        tokio::spawn(async move {});
    }
}
