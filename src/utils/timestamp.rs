use std::time::Instant;

use serde_json::json;

use crate::smr::smr_types::Step;

#[derive(Clone, Debug)]
pub struct Timestamp(Instant);

impl Timestamp {
    pub fn new() -> Timestamp {
        Timestamp(Instant::now())
    }

    pub fn update(&mut self, step: Step) {
        log::info!(
            "{:?}",
            json!({
                "step": step,
                "consume":  Instant::now() - self.0,
            })
        );
        self.0 = Instant::now();
    }
}
