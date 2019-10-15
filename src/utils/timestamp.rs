use std::time::Instant;

use serde_json::json;

use crate::utils::metrics;
use crate::smr::smr_types::Step;

#[derive(Clone, Debug)]
pub struct Timestamp(Instant);

impl Timestamp {
    pub fn new() -> Timestamp {
        Timestamp(Instant::now())
    }

    pub fn update(&mut self, step: Step) {
        metrics(json!({
            "overlord-step": step,
            "cost":  Instant::now() - self.0,
        }));
        self.0 = Instant::now();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_timestamp() {
        let mut ts = Timestamp::new();
        ts.update(Step::Propose);
        ts.update(Step::Prevote);
        ts.update(Step::Precommit);
        ts.update(Step::Commit);
        ts.update(Step::Propose);
    }
}
