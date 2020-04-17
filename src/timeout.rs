#![allow(unused_imports)]

use std::task::{Context, Poll};
use std::time::Duration;
use std::{future::Future, pin::Pin};

use derive_more::Display;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::{FutureExt, SinkExt};
use futures_timer::Delay;

use crate::state::{Stage, Step};
use crate::types::TimeConfig;
use crate::Height;
use std::sync::mpsc::RecvTimeoutError::Timeout;

#[derive(Clone, Debug, Display)]
pub enum TimeoutEvent {
    #[display(fmt = "ProposeTimeout( {} )", _0)]
    ProposeTimeout(Stage),
    #[display(fmt = "PreVoteTimeout( {} )", _0)]
    PreVoteTimeout(Stage),
    #[display(fmt = "PreCommitTimeout( {} )", _0)]
    PreCommitTimeout(Stage),
    #[display(fmt = "BrakeTimeout( {} )", _0)]
    BrakeTimeout(Stage),
    #[display(fmt = "HeightTimeout( {} )", _0)]
    HeightTimeout(Stage),
}

impl From<Stage> for TimeoutEvent {
    fn from(stage: Stage) -> TimeoutEvent {
        match stage.step.clone() {
            Step::Propose => TimeoutEvent::ProposeTimeout(stage),
            Step::PreVote => TimeoutEvent::PreVoteTimeout(stage),
            Step::PreCommit => TimeoutEvent::PreCommitTimeout(stage),
            Step::Brake => TimeoutEvent::BrakeTimeout(stage),
            Step::Commit => TimeoutEvent::HeightTimeout(stage),
        }
    }
}

#[derive(Debug, Display)]
#[display(fmt = "{}", event)]
pub struct TimeoutInfo {
    delay:  Delay,
    event:  TimeoutEvent,
    to_smr: UnboundedSender<TimeoutEvent>,
}

impl Future for TimeoutInfo {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let event = self.event.clone();
        let mut to_smr = self.to_smr.clone();

        match self.delay.poll_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                tokio::spawn(async move {
                    let _ = to_smr.send(event).await;
                });
                Poll::Ready(())
            }
        }
    }
}

impl TimeoutInfo {
    pub fn new(
        interval: Duration,
        event: TimeoutEvent,
        to_smr: UnboundedSender<TimeoutEvent>,
    ) -> Self {
        TimeoutInfo {
            delay: Delay::new(interval),
            event,
            to_smr,
        }
    }
}
