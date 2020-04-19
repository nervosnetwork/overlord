mod common;

use std::io::Write;
use std::thread;
use std::time::Duration;

use chrono::Local;
use log::LevelFilter;

use crate::common::platform::Platform;

#[tokio::test(threaded_scheduler)]
async fn test_1_node() {
    set_log(LevelFilter::Info);
    let platform = Platform::new(1);
    platform.run();
    thread::sleep(Duration::from_secs(10));
}

#[tokio::test(threaded_scheduler)]
async fn test_4_nodes() {
    set_log(LevelFilter::Info);
    let platform = Platform::new(4);
    platform.run();
    thread::sleep(Duration::from_secs(10));
}

fn set_log(level: LevelFilter) {
    let _ = env_logger::builder()
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S.%f"),
                record.level(),
                record.args()
            )
        })
        .filter_level(level)
        .is_test(true)
        .try_init();
}
