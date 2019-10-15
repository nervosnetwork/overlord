pub use log::{debug, error, info, trace, warn};
use log::{log_enabled, Level};

///
#[macro_export]
macro_rules! duration_since_ms {
    ($start:expr) => {{
        let end = Instant::now();
        end.duration_since($start).as_millis() as u64
    }};
}

///
#[macro_export(local_inner_macros)]
macro_rules! json {
    ($($json:tt)+) => {
        serde_json::json!({"timestamp": timestamp(), $($json)+})
    }
}

///
#[macro_export(local_inner_macros)]
macro_rules! metrics {
    ($name:expr => $start:expr, $($json:tt)+) => {
        if metrics_enabled() {
            let cost_ms = duration_since_ms!($start);
            let timing = json!($name: cost_ms, $($json)+);

            log::trace!("{}", timing);
        }
    };
    ($start:expr, $($json:tt)+) => {{
        metrics!("cost_ms" => $start, $($json)+)
    }};
}

///
pub fn metrics_enabled() -> bool {
    log_enabled!(target: "Muta_Metrics", Level::Trace)
}

///
pub fn timestamp() -> String {
    format!("{}", chrono::Utc::now().timestamp())
}
