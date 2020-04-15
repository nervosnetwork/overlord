#![allow(unused_imports)]

use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;
use std::task::{Context as TaskCx, Poll};
use std::time::Duration;
use std::{future::Future, pin::Pin};

use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::stream::{Stream, StreamExt};
use futures::{FutureExt, SinkExt};
use futures_timer::Delay;
use log::error;

use crate::auth::{AuthFixedConfig, AuthManage};
use crate::cabinet::Cabinet;
use crate::state::{Stage, StateInfo};
use crate::timeout::TimeoutEvent;
use crate::types::{FetchedFullBlock, Proposal, UpdateFrom};
use crate::{Adapter, Address, Blk, CommonHex, OverlordMsg, PriKeyHex, Round, St, TimeConfig, Wal};

const MULTIPLIER_CAP: u32 = 5;

pub type WrappedOverlordMsg<B> = (Context, OverlordMsg<B>);

pub struct StateMachine<A: Adapter<B, S>, B: Blk, S: St> {
    state:       StateInfo,
    time_config: TimeConfig,

    adapter: Arc<A>,
    wal:     Wal,
    cabinet: Cabinet<B>,
    auth:    AuthManage<A, B, S>,

    from_net:     UnboundedReceiver<WrappedOverlordMsg<B>>,
    from_fetch:   UnboundedReceiver<FetchedFullBlock>,
    to_fetch:     UnboundedSender<FetchedFullBlock>,
    from_timeout: UnboundedReceiver<TimeoutEvent>,
    to_timeout:   UnboundedSender<TimeoutEvent>,

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
        wal_path: &str,
    ) -> Self {
        let (to_fetch, from_fetch) = unbounded();
        let (to_timeout, from_timeout) = unbounded();
        StateMachine {
            state: StateInfo::default(),
            time_config: TimeConfig::default(),
            adapter: Arc::<A>::clone(adapter),
            wal: Wal::new(wal_path),
            cabinet: Cabinet::default(),
            auth: AuthManage::new(auth_fixed_config),
            from_net: net_receiver,
            from_fetch,
            to_fetch,
            from_timeout,
            to_timeout,
            phantom_s: PhantomData,
        }
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            while let Some(err) = self.next().await {
                // Todo: Add handle error here
                error!("smr error {:?}", err);
            }
        });
    }

    fn process_msg(&mut self, _wrapped_msg: WrappedOverlordMsg<B>) -> Result<(), SMRError> {
        Ok(())
    }

    fn process_fetch(&mut self, _fetched_full_block: FetchedFullBlock) -> Result<(), SMRError> {
        Ok(())
    }

    fn process_timeout(&mut self, _timeout_event: TimeoutEvent) -> Result<(), SMRError> {
        Ok(())
    }
}
impl<A, B, S> Stream for StateMachine<A, B, S>
where
    A: Adapter<B, S>,
    B: Blk,
    S: St,
{
    type Item = SMRError;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskCx) -> Poll<Option<Self::Item>> {
        let mut net_ready = true;
        let mut fetch_ready = true;
        let mut timeout_ready = true;

        loop {
            match self.from_net.poll_next_unpin(cx) {
                Poll::Pending => net_ready = false,

                Poll::Ready(opt) => {
                    let wrapped_msg =
                        opt.expect("Network is down! It's meaningless to continue running");
                    if let Err(e) = self.process_msg(wrapped_msg) {
                        return Poll::Ready(Some(e));
                    }
                }
            };

            match self.from_fetch.poll_next_unpin(cx) {
                Poll::Pending => fetch_ready = false,

                Poll::Ready(opt) => {
                    let fetched_full_block =
                        opt.expect("Fetch Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.process_fetch(fetched_full_block) {
                        return Poll::Ready(Some(e));
                    }
                }
            }

            match self.from_timeout.poll_next_unpin(cx) {
                Poll::Pending => timeout_ready = false,

                Poll::Ready(opt) => {
                    let timeout_event =
                        opt.expect("Timeout Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.process_timeout(timeout_event) {
                        return Poll::Ready(Some(e));
                    }
                }
            }

            if !net_ready && !fetch_ready && !timeout_ready {
                return Poll::Pending;
            }
        }
    }
}

#[derive(Debug, Display)]
pub enum SMRError {
    #[display(fmt = "Other error: {}", _0)]
    Other(String),
}

impl Error for SMRError {}
