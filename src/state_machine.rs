use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::UnboundedReceiver;

use crate::auth_manage::AuthManage;
use crate::cabinet::Cabinet;
use crate::{Adapter, Address, Blk, CommonHex, OverlordMsg, PriKeyHex, St, Wal};

pub struct StateMachine<A: Adapter<B, S>, B: Blk, S: St> {
    adapter: Arc<A>,
    network: UnboundedReceiver<(Context, OverlordMsg<B>)>,

    wal:     Wal,
    cabinet: Cabinet<B>,
    auth:    AuthManage<A, B, S>,

    phantom_s: PhantomData<S>,
}

impl<A, B, S> StateMachine<A, B, S>
where
    A: Adapter<B, S>,
    B: Blk,
    S: St,
{
    pub fn new(
        common_ref: CommonHex,
        pri_key: PriKeyHex,
        address: Address,
        adapter: &Arc<A>,
        receiver: UnboundedReceiver<(Context, OverlordMsg<B>)>,
        wal_path: &str,
    ) -> Self {
        StateMachine {
            adapter:   Arc::<A>::clone(adapter),
            network:   receiver,
            wal:       Wal::new(wal_path),
            cabinet:   Cabinet::default(),
            auth:      AuthManage::new(common_ref, pri_key, address),
            phantom_s: PhantomData,
        }
    }

    pub fn run(&self) {
        tokio::spawn(async move {});
    }
}
