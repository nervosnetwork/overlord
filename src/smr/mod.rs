#![allow(dead_code)]

///
pub mod smr_types;
///
mod state_machine;
///
#[cfg(test)]
mod tests;

use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::{Stream, StreamExt};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

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

///
#[derive(Clone, Debug)]
pub struct SMR {
    tx: UnboundedSender<SMRTrigger>,
}

impl SMR {
    pub fn new(sender: UnboundedSender<SMRTrigger>) -> Self {
        SMR { tx: sender }
    }

    /// A function to touch off SMR trigger gate.
    pub fn trigger(&mut self, gate: SMRTrigger) -> ConsensusResult<()> {
        let trigger_type = gate.trigger_type.clone().to_string();
        self.tx
            .try_send(gate)
            .map_err(|_| ConsensusError::TriggerSMRErr(trigger_type))?;
        Ok(())
    }

    /// Trigger SMR to goto a new epoch.
    pub fn new_epoch(&mut self, epoch_id: u64) -> ConsensusResult<()> {
        let trigger = TriggerType::NewEpoch(epoch_id);
        self.tx
            .try_send(SMRTrigger {
                trigger_type: trigger.clone(),
                source:       TriggerSource::State,
                hash:         Hash::new(),
                round:        None,
            })
            .map_err(|_| ConsensusError::TriggerSMRErr(trigger.to_string()))?;
        Ok(())
    }
}

///
#[derive(Debug)]
pub struct Event {
    rx: UnboundedReceiver<SMREvent>,
}

impl Stream for Event {
    type Item = ConsensusResult<SMREvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.rx.poll_next_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(msg) => {
                Poll::Ready(Some(msg.ok_or_else(|| {
                    ConsensusError::MonitorEventErr("Sender has dropped".to_string())
                })))
            }
        }
    }
}

impl Event {
    pub fn new(receiver: UnboundedReceiver<SMREvent>) -> Self {
        Event { rx: receiver }
    }

    pub async fn recv(&mut self) -> ConsensusResult<SMREvent> {
        if let Some(event) = self.rx.recv().await {
            Ok(event)
        } else {
            Err(ConsensusError::MonitorEventErr(
                "Sender has dropped".to_string(),
            ))
        }
    }
}
