use tokio::sync::{mpsc::UnboundedReceiver, watch::Sender};

use crate::error::ConsensusError;
use crate::smr::smr_types::{SMREvent, SMRTrigger, Step, TriggerType};
use crate::types::{Hash, PoLC};
use crate::{ConsensusResult, INIT_EPOCH_ID, INIT_ROUND};

/// A smallest implementation of an atomic overlord state machine. It
pub struct StateMachine {
    epoch_id:      u64,
    round:         u64,
    step:          Step,
    proposal_hash: Option<Hash>,
    lock:          Option<PoLC>,

    event:   Sender<SMREvent>,
    trigger: UnboundedReceiver<SMRTrigger>,
}

impl StateMachine {
    pub fn new(
        event_sender: Sender<SMREvent>,
        trigger_receiver: UnboundedReceiver<SMRTrigger>,
    ) -> Self {
        StateMachine {
            epoch_id:      INIT_EPOCH_ID,
            round:         INIT_ROUND,
            step:          Step::default(),
            proposal_hash: None,
            lock:          None,
            trigger:       trigger_receiver,
            event:         event_sender,
        }
    }

    async fn process_events() {}
}
