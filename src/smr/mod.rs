///
pub mod smr_types;
///
mod state_machine;
///
#[cfg(test)]
mod tests;

use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::stream::{FusedStream, Stream, StreamExt};
use log::error;

use crate::smr::smr_types::{SMREvent, SMRStatus, SMRTrigger, TriggerSource, TriggerType};
use crate::smr::state_machine::StateMachine;
use crate::types::Hash;
use crate::{error::ConsensusError, ConsensusResult};

///
#[derive(Debug)]
pub struct SMR {
    smr_handler:   Option<SMRHandler>,
    state_machine: StateMachine,
}

impl SMR {
    pub fn new() -> (Self, Event, Event) {
        let (tx, rx) = unbounded();
        let smr = SMRHandler::new(tx);
        let (state_machine, evt_state, evt_timer) = StateMachine::new(rx);

        let provider = SMR {
            smr_handler: Some(smr),
            state_machine,
        };

        (provider, evt_state, evt_timer)
    }

    /// Take the SMR handler and this function will be called only once.
    pub fn take_smr(&mut self) -> SMRHandler {
        assert!(self.smr_handler.is_some());
        self.smr_handler.take().unwrap()
    }

    /// Run SMR module in tokio environment.
    pub fn run(mut self) {
        tokio::spawn(async move {
            loop {
                let res = self.state_machine.next().await;
                if let Some(err) = res {
                    error!("Overlord: SMR error {:?}", err);
                }
            }
        });
    }
}

///
#[derive(Clone, Debug)]
pub struct SMRHandler {
    tx: UnboundedSender<SMRTrigger>,
}

impl SMRHandler {
    /// Create a new SMR.
    pub fn new(sender: UnboundedSender<SMRTrigger>) -> Self {
        SMRHandler { tx: sender }
    }

    /// A function to touch off SMR trigger gate.
    pub fn trigger(&mut self, gate: SMRTrigger) -> ConsensusResult<()> {
        let trigger_type = gate.trigger_type.clone().to_string();
        self.tx
            .unbounded_send(gate)
            .map_err(|_| ConsensusError::TriggerSMRErr(trigger_type))
    }

    /// Trigger SMR to goto a new height.
    pub fn new_height_status(&mut self, status: SMRStatus) -> ConsensusResult<()> {
        let height = status.height;
        let trigger = TriggerType::NewHeight(status);
        self.tx
            .unbounded_send(SMRTrigger {
                trigger_type: trigger.clone(),
                source: TriggerSource::State,
                hash: Hash::new(),
                round: None,
                height,
                wal_info: None,
            })
            .map_err(|_| ConsensusError::TriggerSMRErr(trigger.to_string()))
    }
}

///
#[derive(Debug)]
pub struct Event {
    rx: UnboundedReceiver<SMREvent>,
}

impl Stream for Event {
    type Item = SMREvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.rx.poll_next_unpin(cx)
    }
}

impl FusedStream for Event {
    fn is_terminated(&self) -> bool {
        self.rx.is_terminated()
    }
}

impl Event {
    pub fn new(receiver: UnboundedReceiver<SMREvent>) -> Self {
        Event { rx: receiver }
    }

    #[cfg(test)]
    pub fn close(&mut self) {
        self.rx.close();
    }
}
