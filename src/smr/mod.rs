#![allow(dead_code)]

///
mod smr_types;
///
mod state_machine;

use bytes::Bytes;
use tokio::sync::{mpsc::UnboundedSender, watch::Receiver};

use crate::smr::smr_types::{SMREvent, SMRTrigger, TriggerType};
use crate::{error::ConsensusError, ConsensusResult};

///
#[derive(Clone)]
pub struct SMR(UnboundedSender<SMRTrigger>);

impl SMR {
    /// A function to touch off SMR trigger gate.
    pub fn trigger(&mut self, gate: SMRTrigger) -> ConsensusResult<()> {
        let trigger_type: u8 = gate.trigger_type.clone().into();
        self.0
            .try_send(gate)
            .map_err(|_| ConsensusError::TriggerSMRErr(trigger_type))?;
        Ok(())
    }

    /// Trigger SMR to goto a new epoch.
    pub fn new_epoch(&mut self, epoch_id: u64) -> ConsensusResult<()> {
        self.0
            .try_send(SMRTrigger {
                trigger_type: TriggerType::NewEpoch(epoch_id),
                hash:         Bytes::new(),
                round:        None,
            })
            .map_err(|_| ConsensusError::TriggerSMRErr(3u8))?;
        Ok(())
    }
}

///
pub struct Event(Receiver<SMREvent>);

impl Event {
    pub async fn recv(&mut self) -> ConsensusResult<SMREvent> {
        if let Some(event) = self.0.recv().await {
            Ok(event)
        } else {
            Err(ConsensusError::MonitorEventErr(
                "Sender has dropped".to_string(),
            ))
        }
    }
}
