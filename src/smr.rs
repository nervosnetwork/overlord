#![allow(unused_imports)]

use std::marker::PhantomData;
use std::sync::Arc;
use std::task::{Context as TaskCx, Poll};
use std::time::Duration;
use std::{future::Future, pin::Pin};

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::stream::{Stream, StreamExt};
use futures::{FutureExt, SinkExt};
use futures_timer::Delay;

use crate::auth::{AuthFixedConfig, AuthManage};
use crate::cabinet::Cabinet;
use crate::state::{Stage, StateInfo};
use crate::timeout::TimeoutEvent;
use crate::types::{FetchFullBlock, Proposal, UpdateFrom};
use crate::{Adapter, Address, Blk, CommonHex, OverlordMsg, PriKeyHex, Round, St, TimeConfig, Wal};

const MULTIPLIER_CAP: u32 = 5;

pub struct StateMachine<A: Adapter<B, S>, B: Blk, S: St> {
    state:       StateInfo,
    time_config: TimeConfig,

    adapter: Arc<A>,
    wal:     Wal,
    cabinet: Cabinet<B>,
    auth:    AuthManage<A, B, S>,

    from_net:     UnboundedReceiver<(Context, OverlordMsg<B>)>,
    from_fetch:   UnboundedReceiver<FetchFullBlock>,
    to_fetch:     UnboundedSender<FetchFullBlock>,
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

    pub fn run(&self) {
        tokio::spawn(async move {});
    }
}
// impl Stream for StateMachine {
//     type Item = ConsensusError;
//
//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
//         let mut event_ready = true;
//         let mut timer_ready = true;
//
//         loop {
//             match self.event.poll_next_unpin(cx) {
//                 Poll::Pending => event_ready = false,
//
//                 Poll::Ready(event) => {
//                     if event.is_none() {
//                         return Poll::Ready(Some(ConsensusError::TimerErr(
//                             "Channel dropped".to_string(),
//                         )));
//                     }
//
//                     let event = event.unwrap();
//                     if event == SMREvent::Stop {
//                         return Poll::Ready(None);
//                     }
//                     if let Err(e) = self.set_timer(event) {
//                         return Poll::Ready(Some(e));
//                     }
//                 }
//             };
//
//             match self.notify.poll_next_unpin(cx) {
//                 Poll::Pending => timer_ready = false,
//
//                 Poll::Ready(event) => {
//                     if event.is_none() {
//                         return Poll::Ready(Some(ConsensusError::TimerErr(
//                             "Channel terminated".to_string(),
//                         )));
//                     }
//
//                     let event = event.unwrap();
//                     if let Err(e) = self.trigger(event) {
//                         return Poll::Ready(Some(e));
//                     }
//                 }
//             }
//             if !event_ready && !timer_ready {
//                 return Poll::Pending;
//             }
//         }
//     }
// }
