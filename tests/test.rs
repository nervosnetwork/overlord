mod common;

use std::io::Write;
use std::thread;
use std::time::Duration;

use chrono::Local;
use log::LevelFilter;

use crate::common::platform::Platform;

#[tokio::test(threaded_scheduler)]
async fn test_1_node() {
    set_log(LevelFilter::Error);
    Platform::new(1);
    thread::sleep(Duration::from_secs(10));
}

#[tokio::test(threaded_scheduler)]
async fn test_4_nodes() {
    set_log(LevelFilter::Info);
    Platform::new(4);
    thread::sleep(Duration::from_secs(10));
}

#[tokio::test(threaded_scheduler)]
async fn test_wal_1_node() {
    set_log(LevelFilter::Info);
    let platform = Platform::new(1);
    thread::sleep(Duration::from_secs(3));
    let vec = platform.get_address_list();
    let address = vec.get(0).unwrap();
    // stop node
    platform.stop_node(&address);
    thread::sleep(Duration::from_secs(1));
    let height_after_stop = platform.get_latest_height(&address);
    thread::sleep(Duration::from_secs(3));
    let height_after_sleep = platform.get_latest_height(&address);
    assert_eq!(height_after_stop, height_after_sleep);

    // restart node
    platform.restart_node(&address);
    thread::sleep(Duration::from_secs(3));
    let height_after_restart = platform.get_latest_height(&address);
    assert!(
        height_after_restart > height_after_sleep,
        "{} > {}",
        height_after_restart,
        height_after_sleep
    );
}

#[tokio::test(threaded_scheduler)]
async fn test_wal_4_nodes() {
    // set_log(LevelFilter::Debug);
    let platform = Platform::new(4);
    thread::sleep(Duration::from_secs(3));
    let vec = platform.get_address_list();
    let address = vec.get(0).unwrap();
    // stop node
    platform.stop_node(&address);
    thread::sleep(Duration::from_secs(1));
    let height_after_stop = platform.get_latest_height(&address);
    thread::sleep(Duration::from_secs(3));
    let height_after_sleep = platform.get_latest_height(&address);
    assert_eq!(height_after_stop, height_after_sleep);

    // restart node
    platform.restart_node(&address);
    thread::sleep(Duration::from_secs(100));
    let height_after_restart = platform.get_latest_height(&address);
    assert!(
        height_after_restart > height_after_sleep,
        "{} > {}",
        height_after_restart,
        height_after_sleep
    );
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
