#![allow(unused_imports)]

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::event::{SMREvent, TimerEvent};
use crate::state::Stage;
use crate::types::TimeConfig;
use crate::{Height, Round};

pub struct Timer {
    stage:  Stage,
    config: TimeConfig,

    from_smr: UnboundedReceiver<SMREvent>,
    to_smr:   UnboundedSender<TimerEvent>,
}

impl Timer {
    pub fn new(from_smr: UnboundedReceiver<SMREvent>, to_smr: UnboundedSender<TimerEvent>) -> Self {
        Timer {
            stage: Stage::default(),
            config: TimeConfig::default(),
            from_smr,
            to_smr,
        }
    }

    pub fn run(&self) {
        tokio::spawn(async move {});
    }
}
