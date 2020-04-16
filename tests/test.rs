mod common;

use std::thread;
use std::time::Duration;

use log::LevelFilter;

use crate::common::platform::Platform;

#[tokio::test(threaded_scheduler)]
async fn test_4_nodes() {
    let _ = env_logger::builder()
        .filter_level(LevelFilter::Warn)
        .is_test(true)
        .try_init();
    let platform = Platform::new(4);
    platform.run();
    thread::sleep(Duration::from_secs(10));
}
