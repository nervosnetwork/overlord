use std::task::{Context as TaskContext, Poll};
use std::time::Duration;
use std::{future::Future, pin::Pin};

use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::UnboundedSender;
use futures::{FutureExt, SinkExt};
use futures_timer::Delay;

use crate::state::{Stage, Step};
use crate::types::TinyHex;
use crate::utils::agent::ChannelMsg;
use crate::{Address, Blk, FullBlk, St};

#[derive(Clone, Debug, Display)]
pub enum TimeoutEvent {
    #[display(fmt = "propose_timeout: {}", _0)]
    ProposeTimeout(Stage),
    #[display(fmt = "pre_vote_timeout: {}", _0)]
    PreVoteTimeout(Stage),
    #[display(fmt = "pre_commit_timeout: {}", _0)]
    PreCommitTimeout(Stage),
    #[display(fmt = "brake_timeout: {}", _0)]
    BrakeTimeout(Stage),
    #[display(fmt = "next_height_timeout")]
    NextHeightTimeout,
    #[display(fmt = "height_timeout")]
    HeightTimeout,
    #[display(fmt = "sync_timeout: {}", _0)]
    SyncTimeout(u64),
    #[display(fmt = "clear_timeout: {}", "_0.tiny_hex()")]
    ClearTimeout(Address),
}

impl From<Stage> for TimeoutEvent {
    fn from(stage: Stage) -> TimeoutEvent {
        match stage.step.clone() {
            Step::Propose => TimeoutEvent::ProposeTimeout(stage),
            Step::PreVote => TimeoutEvent::PreVoteTimeout(stage),
            Step::PreCommit => TimeoutEvent::PreCommitTimeout(stage),
            Step::Brake => TimeoutEvent::BrakeTimeout(stage),
            Step::Commit => TimeoutEvent::NextHeightTimeout,
        }
    }
}

#[derive(Debug, Display)]
#[display(fmt = "{}", event)]
pub struct TimeoutInfo<B: Blk, F: FullBlk<B>, S: St> {
    pub delay: Delay,
    ctx:       Context,
    event:     TimeoutEvent,
    sender:    UnboundedSender<ChannelMsg<B, F, S>>,
}

impl<B: Blk, F: FullBlk<B>, S: St> Future for TimeoutInfo<B, F, S> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext) -> Poll<Self::Output> {
        let event = self.event.clone();
        let ctx = self.ctx.clone();
        let mut sender = self.sender.clone();

        match self.delay.poll_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                tokio::spawn(async move {
                    let _ = sender.send(ChannelMsg::TimeoutEvent(ctx, event)).await;
                });
                Poll::Ready(())
            }
        }
    }
}

impl<B: Blk, F: FullBlk<B>, S: St> TimeoutInfo<B, F, S> {
    pub fn new(
        ctx: Context,
        delay: Duration,
        event: TimeoutEvent,
        sender: UnboundedSender<ChannelMsg<B, F, S>>,
    ) -> Self {
        TimeoutInfo {
            ctx,
            delay: Delay::new(delay),
            event,
            sender,
        }
    }
}
