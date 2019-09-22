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

use crate::smr::smr_types::{SMREvent, SMRTrigger, TriggerSource, TriggerType};
use crate::smr::state_machine::StateMachine;
use crate::types::Hash;
use crate::{error::ConsensusError, ConsensusResult};

///
#[derive(Debug)]
pub struct SMRProvider {
    smr:           Option<SMR>,
    state_machine: StateMachine,
}

impl SMRProvider {
    pub fn new() -> (Self, Event, Event) {
        let (tx, rx) = unbounded();
        let smr = SMR::new(tx);
        let (state_machine, evt_1, evt_2) = StateMachine::new(rx);

        let provider = SMRProvider {
            smr: Some(smr),
            state_machine,
        };

        (provider, evt_1, evt_2)
    }

    /// Take the SMR handler and this function will be called only once.
    pub fn take_smr(&mut self) -> SMR {
        assert!(self.smr.is_some());
        self.smr.take().unwrap()
    }

    /// Run SMR module in runtime environment.
    pub fn run(mut self) {
        runtime::spawn(async move {
            loop {
                let _ = self.state_machine.next().await;
            }
        });
    }
}

///
#[derive(Clone, Debug)]
pub struct SMR {
    tx: UnboundedSender<SMRTrigger>,
}

impl SMR {
    /// Create a new SMR.
    pub fn new(sender: UnboundedSender<SMRTrigger>) -> Self {
        SMR { tx: sender }
    }

    /// A function to touch off SMR trigger gate.
    pub fn trigger(&mut self, gate: SMRTrigger) -> ConsensusResult<()> {
        let trigger_type = gate.trigger_type.clone().to_string();
        self.tx
            .unbounded_send(gate)
            .map_err(|_| ConsensusError::TriggerSMRErr(trigger_type))
    }

    /// Trigger SMR to goto a new epoch.
    pub fn new_epoch(&mut self, epoch_id: u64) -> ConsensusResult<()> {
        // TODO refactor
        let trigger = TriggerType::NewEpoch(epoch_id);
        self.tx
            .unbounded_send(SMRTrigger {
                trigger_type: trigger.clone(),
                source: TriggerSource::State,
                hash: Hash::new(),
                round: None,
                epoch_id,
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
}
