use log::{log_enabled, Level};

///
#[macro_export(local_inner_macros)]
macro_rules! json {
    ($event:expr, $($json:tt)+) => {
        serde_json::json!({"event": $event.to_string(), "timestamp": timestamp(), $($json)+})
    }
}

///
#[macro_export(local_inner_macros)]
macro_rules! metrics {
    ($event:expr, $($json:tt)+) => {
        if metrics_enabled() {
            let info = json!($event, $($json)+);
            log::trace!("{}", info);
        }
    };
}

pub fn metrics_enabled() -> bool {
    log_enabled!(target: "Overlord_Metrics", Level::Trace)
}

pub fn timestamp() -> String {
    format!("{}", chrono::Utc::now().timestamp())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_metrics() {
        env_logger::init();

        metrics!("commit", "hash": hex::encode(vec![1, 2, 3, 15, 16]));
    }
}
