#![allow(dead_code)]

///
pub mod smr_types;
///
mod state_machine;

use tokio::sync::{mpsc::UnboundedSender, watch::Receiver};

use crate::smr::smr_types::{SMREvent, SMRTrigger, TriggerSource, TriggerType};
use crate::smr::state_machine::StateMachine;
use crate::types::Hash;
use crate::{error::ConsensusError, ConsensusResult};

///
pub struct SMRProvider {
    smr:           Option<SMR>,
    state_machine: StateMachine,
}

///
#[derive(Clone)]
pub struct SMR(UnboundedSender<SMRTrigger>);

impl SMR {
    /// A function to touch off SMR trigger gate.
    pub fn trigger(&mut self, gate: SMRTrigger) -> ConsensusResult<()> {
        let trigger_type = gate.trigger_type.clone().to_string();
        self.0
            .try_send(gate)
            .map_err(|_| ConsensusError::TriggerSMRErr(trigger_type))?;
        Ok(())
    }

    /// Trigger SMR to goto a new epoch.
    pub fn new_epoch(&mut self, epoch_id: u64) -> ConsensusResult<()> {
        let trigger = TriggerType::NewEpoch(epoch_id);
        self.0
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
