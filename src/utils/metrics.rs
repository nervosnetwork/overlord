use std::time::{SystemTime, UNIX_EPOCH};

use log::trace;
use serde_json::{json, Value};

/// Log the context message and a json value in json format at the trace level.
pub fn metrics(info_value: Value) {
    let output = json!({"info": info_value, "timestamp": sys_now()});
    trace!("{}", output);
}

fn sys_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod test {
    use super::*;
    use env_logger;

    #[test]
    fn test_log() {
        env_logger::init();

        let log = json!({"epoch_id": 0});
        metrics(log);
    }
}
