mod common;

use std::io::Write;
use std::thread;
use std::time::Duration;

use chrono::Local;
use log::LevelFilter;
use overlord::{Address, TinyHex};
use rand::Rng;

use crate::common::platform::Platform;

#[tokio::test(threaded_scheduler)]
async fn test_1_node() {
    set_log(LevelFilter::Error);
    Platform::new(1);
    thread::sleep(Duration::from_secs(10));
}

#[tokio::test(threaded_scheduler)]
async fn test_4_nodes() {
    // set_log(LevelFilter::Info);
    Platform::new(4);
    thread::sleep(Duration::from_secs(10));
}

#[tokio::test(threaded_scheduler)]
async fn test_wal_1_node() {
    // set_log(LevelFilter::Info);
    let platform = Platform::new(1);
    thread::sleep(Duration::from_secs(3));
    let vec = platform.get_address_list();
    let address = vec.get(0).unwrap();
    // stop node
    platform.stop_node(&address);

    // restart node
    platform.restart_node(&address);
    thread::sleep(Duration::from_secs(5));
}

#[tokio::test(threaded_scheduler)]
async fn test_wal_4_nodes() {
    // set_log(LevelFilter::Debug);
    let platform = Platform::new(4);
    platform.update_interval(200);
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
    thread::sleep(Duration::from_secs(5));
}

#[tokio::test(threaded_scheduler)]
async fn test_sync() {
    let mut platform = Platform::new(1);
    platform.update_interval(100);
    thread::sleep(Duration::from_secs(3));
    for _ in 0..10 {
        platform.add_new_nodes(1);
        thread::sleep(Duration::from_secs(5));
    }
}

#[tokio::test(threaded_scheduler)]
async fn test_update_nodes() {
    // set_log(LevelFilter::Debug);
    let platform = Platform::new(3);
    platform.update_interval(100);
    thread::sleep(Duration::from_secs(3));

    let mut nodes: Vec<(Address, bool)> = platform
        .get_address_list()
        .into_iter()
        .map(|a| (a, true))
        .collect();

    for i in 0..10 {
        let index = rand::thread_rng().gen::<usize>() % 3;
        let (address, is_auth) = nodes.get_mut(index).unwrap();
        if *is_auth {
            platform.remove_node_auth(address);
            println!("###### remove auth {}", address.tiny_hex());
            *is_auth = false;
        } else {
            platform.auth_new_node(address);
            println!("###### auth {}", address.tiny_hex());
            *is_auth = true;
        }
        thread::sleep(Duration::from_secs(5));
        println!("###### CASE {} end", i);
    }
}

#[tokio::test(threaded_scheduler)]
async fn test_update_interval() {
    let platform = Platform::new(4);
    thread::sleep(Duration::from_secs(10));
    platform.update_interval(100);
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
