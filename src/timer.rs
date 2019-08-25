#![allow(dead_code)]

use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use std::{future::Future, pin::Pin};

use futures::stream::{Stream, StreamExt};
use futures::{FutureExt, SinkExt};
use futures_timer::{Delay, TimerHandle};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::watch::Receiver;
// use tokio::timer::Delay;

use crate::smr::smr_types::{SMREvent, SMRTrigger, TriggerSource, TriggerType};
use crate::{error::ConsensusError, ConsensusResult};
use crate::{smr::SMR, INIT_ROUND};
use crate::{types::Hash, utils::timer_config::TimerConfig};

/// Overlord timer used futures timer which is powered by a timer heap. When monitor a SMR event,
/// timer will get timeout interval from timer config, then set a delay. When the timeout expires,
#[derive(Debug)]
pub struct Timer {
    config: TimerConfig,
    event:  Receiver<SMREvent>,
    sender: UnboundedSender<SMREvent>,
    notify: UnboundedReceiver<SMREvent>,
    smr:    SMR,
    round:  u64,
}

///
impl Stream for Timer {
    type Item = ConsensusError;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut event_ready = true;
        let mut timer_ready = true;

        loop {
            match self.event.poll_next_unpin(cx) {
                Poll::Pending => event_ready = false,

                Poll::Ready(event) => {
                    if event.is_none() {
                        return Poll::Ready(Some(ConsensusError::TimerErr(
                            "Channel dropped".to_string(),
                        )));
                    }

                    let event = event.unwrap();
                    println!("{:?}", event);
                    if event == SMREvent::Stop {
                        return Poll::Ready(None);
                    }
                    if let Err(e) = self.set_timer(event) {
                        return Poll::Ready(Some(e));
                    }
                }
            };

            match self.notify.poll_next_unpin(cx) {
                Poll::Pending => timer_ready = false,

                Poll::Ready(event) => {
                    if event.is_none() {
                        return Poll::Ready(Some(ConsensusError::TimerErr(
                            "Channel terminated".to_string(),
                        )));
                    }

                    let event = event.unwrap();
                    if let Err(e) = self.trigger(event) {
                        return Poll::Ready(Some(e));
                    }
                }
            }
            if !event_ready && !timer_ready {
                return Poll::Pending;
            }
        }
    }
}

impl Timer {
    pub fn new(event_monitor: Receiver<SMREvent>, smr: SMR, interval: u64) -> Self {
        let (tx, rx) = unbounded_channel();
        Timer {
            config: TimerConfig::new(interval),
            event: event_monitor,
            sender: tx,
            notify: rx,
            round: INIT_ROUND,
            smr,
        }
    }

    fn set_timer(&mut self, event: SMREvent) -> ConsensusResult<()> {
        match event.clone() {
            SMREvent::NewRoundInfo { round, .. } => self.round = round,
            SMREvent::Commit(_) => return Ok(()),
            _ => (),
        };

        let interval = self.config.get_timeout(event.clone())?;
        let smr_timer = TimeoutInfo::new(interval, event, self.sender.clone());
        tokio::spawn(async move {
            smr_timer.await;
        });
        Ok(())
    }

    fn trigger(&mut self, event: SMREvent) -> ConsensusResult<()> {
        let (trigger_type, round) = match event {
            SMREvent::NewRoundInfo { .. } => (TriggerType::Proposal, None),
            SMREvent::PrevoteVote(_) => (TriggerType::PrevoteQC, Some(self.round)),
            SMREvent::PrecommitVote(_) => (TriggerType::PrecommitQC, Some(self.round)),
            _ => return Err(ConsensusError::TimerErr("No commit timer".to_string())),
        };

        self.smr.trigger(SMRTrigger {
            source: TriggerSource::Timer,
            hash: Hash::new(),
            trigger_type,
            round,
        })
    }
}

/// Timeout info which is a future consists of a `futures-timer Delay`, timeout info and a sender.
/// When the timeout expires, future will send timeout info by sender.
#[derive(Debug)]
struct TimeoutInfo {
    timeout: Delay,
    info:    SMREvent,
    sender:  UnboundedSender<SMREvent>,
}

impl Future for TimeoutInfo {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let msg = self.info.clone();
        let mut tx = self.sender.clone();

        match self.timeout.poll_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                tokio::spawn(async move {
                    let _ = tx.send(msg).await;
                });
                Poll::Ready(())
            }
        }
    }
}

impl TimeoutInfo {
    fn new(interval: Duration, event: SMREvent, tx: UnboundedSender<SMREvent>) -> Self {
        let mut delay = Delay::new_handle(Instant::now(), TimerHandle::default());
        delay.reset(interval);

        TimeoutInfo {
            timeout: delay,
            info:    event,
            sender:  tx,
        }
    }
}

#[cfg(test)]
mod test {
    use std::{thread, time::Duration};

    use futures::stream::StreamExt;
    use tokio::runtime::Runtime;
    use tokio::sync::{mpsc::unbounded_channel, watch::channel};

    use crate::smr::smr_types::{SMREvent, SMRTrigger, TriggerSource, TriggerType};
    use crate::types::Hash;
    use crate::{smr::SMR, timer::Timer};

    fn test_timer_trigger(input: SMREvent, output: SMRTrigger) {
        let (trigger_tx, mut trigger_rx) = unbounded_channel();
        let (event_tx, event_rx) = channel(SMREvent::Commit(Hash::new()));
        let mut timer = Timer::new(event_rx, SMR::new(trigger_tx), 3000);
        event_tx.broadcast(input).unwrap();

        let rt = Runtime::new().unwrap();
        rt.spawn(async move {
            loop {
                match timer.next().await {
                    None => break,
                    Some(_) => panic!("Error"),
                }
            }
        });

        rt.block_on(async move {
            if let Some(res) = trigger_rx.recv().await {
                assert_eq!(res, output);
                event_tx.broadcast(SMREvent::Stop).unwrap();
            }
        })
    }

    fn gen_output(trigger_type: TriggerType, round: Option<u64>) -> SMRTrigger {
        SMRTrigger {
            source: TriggerSource::Timer,
            hash: Hash::new(),
            trigger_type,
            round,
        }
    }

    #[test]
    fn test_correctness() {
        // Test propose step timer.
        test_timer_trigger(
            SMREvent::NewRoundInfo {
                round:         0,
                lock_round:    None,
                lock_proposal: None,
            },
            gen_output(TriggerType::Proposal, None),
        );

        // Test prevote step timer.
        test_timer_trigger(
            SMREvent::PrevoteVote(Hash::new()),
            gen_output(TriggerType::PrevoteQC, Some(0)),
        );

        // Test precommit step timer.
        test_timer_trigger(
            SMREvent::PrecommitVote(Hash::new()),
            gen_output(TriggerType::PrecommitQC, Some(0)),
        );
    }

    #[test]
    fn test_order() {
        let (trigger_tx, mut trigger_rx) = unbounded_channel();
        let (event_tx, event_rx) = channel(SMREvent::Commit(Hash::new()));
        let mut timer = Timer::new(event_rx, SMR::new(trigger_tx), 3000);

        let new_round_event = SMREvent::NewRoundInfo {
            round:         0,
            lock_round:    None,
            lock_proposal: None,
        };
        let prevote_event = SMREvent::PrevoteVote(Hash::new());
        let precommit_event = SMREvent::PrecommitVote(Hash::new());

        let rt = Runtime::new().unwrap();
        rt.spawn(async move {
            loop {
                match timer.next().await {
                    None => break,
                    Some(_) => panic!("Error"),
                }
            }
        });

        event_tx.broadcast(new_round_event).unwrap();
        thread::sleep(Duration::from_micros(300));
        event_tx.broadcast(prevote_event).unwrap();
        thread::sleep(Duration::from_micros(300));
        event_tx.broadcast(precommit_event).unwrap();

        rt.block_on(async move {
            let mut count = 1u32;
            let mut output = Vec::new();
            let predict = vec![
                gen_output(TriggerType::PrecommitQC, Some(0)),
                gen_output(TriggerType::PrevoteQC, Some(0)),
                gen_output(TriggerType::Proposal, None),
            ];

            while let Some(res) = trigger_rx.recv().await {
                output.push(res);
                if count != 3 {
                    count += 1;
                } else {
                    assert_eq!(predict, output);
                    event_tx.broadcast(SMREvent::Stop).unwrap();
                    return;
                }
            }
        })
    }
}
