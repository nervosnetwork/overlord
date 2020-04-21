mod common;

use std::io::Write;
use std::thread;
use std::time::Duration;

use chrono::Local;
use log::LevelFilter;
use overlord::{Address, TinyHex};

use crate::common::platform::Platform;
use rand::Rng;

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

#[tokio::test(threaded_scheduler)]
async fn test_update_nodes() {
    set_log(LevelFilter::Info);
    let platform = Platform::new(2);
    thread::sleep(Duration::from_secs(10));

    let vec = platform.get_address_list();
    platform.remove_node_auth(&vec[0]);
    thread::sleep(Duration::from_secs(10));

    platform.auth_new_node(&vec[0]);
    thread::sleep(Duration::from_secs(10));
}

#[tokio::test(threaded_scheduler)]
async fn test_update_interval() {
    let platform = Platform::new(4);
    thread::sleep(Duration::from_secs(10));
    platform.update_interval(100);
    thread::sleep(Duration::from_secs(10));
}

const NODE_LIMIT: usize = 8;

#[tokio::test(threaded_scheduler)]
async fn test_chaos() {
    set_log(LevelFilter::Error);
    let init_n = 4;

    let mut platform = Platform::new(init_n);
    platform.update_interval(20);
    thread::sleep(Duration::from_secs(1));

    let mut cnt = 0;
    let mut total_n = init_n;
    let mut alive_n = init_n;
    let mut auth_n = init_n;
    let mut alive_auth_n = init_n;
    let mut alive_nodes: Vec<(Address, bool)> = platform
        .get_address_list()
        .into_iter()
        .map(|address| (address, true))
        .collect();
    println!(
        "[INIT]\n\t<list> {:?}",
        alive_nodes
            .iter()
            .map(|(address, is_auth)| (address.tiny_hex(), *is_auth))
            .collect::<Vec<(String, bool)>>()
    );
    let mut stopped_nodes: Vec<(Address, bool)> = vec![];
    while cnt < 3 {
        let (fortune_auth_n, fortune_alive_n) = fortune_oracle();
        println!("[CASE {}] start\n\t<state> total_n: {}, alive_n: {}\n\t<auth> auth_n: {}, alive_auth_n: {}, auth_list: {:?}\n\t<alive> {:?}\n\t<stopped> {:?}\n\t<fortune> auth_n: {}, alive_n: {}\n",
                 cnt, total_n, alive_n, auth_n, alive_auth_n, platform.get_auth_list(),
                 alive_nodes.iter().map(|(address, bool)| (address.tiny_hex(), *bool)).collect::<Vec<(String, bool)>>(),
                 stopped_nodes.iter().map(|(address, bool)| (address.tiny_hex(), *bool)).collect::<Vec<(String, bool)>>(),
                 fortune_auth_n, fortune_alive_n);
        // do auth first
        if fortune_auth_n > total_n {
            let address_list = platform.add_new_nodes(fortune_auth_n - total_n);
            platform.auth_node_list(&address_list);
            total_n = fortune_auth_n;
            alive_n += address_list.len();
            auth_n = fortune_auth_n;
            alive_auth_n += address_list.len();
            alive_nodes.append(
                &mut address_list
                    .into_iter()
                    .map(|address| (address, true))
                    .collect(),
            );
        } else if fortune_auth_n > auth_n {
            alive_nodes.iter_mut().for_each(|(address, is_auth)| {
                if fortune_auth_n > auth_n && !*is_auth {
                    platform.auth_new_node(address);
                    auth_n += 1;
                    alive_auth_n += 1;
                    *is_auth = true;
                }
            });
            stopped_nodes.iter_mut().for_each(|(address, is_auth)| {
                if fortune_auth_n > auth_n && !*is_auth {
                    platform.auth_new_node(address);
                    auth_n += 1;
                    *is_auth = true;
                }
            });
        } else {
            alive_nodes.iter_mut().for_each(|(address, is_auth)| {
                if fortune_auth_n < auth_n && *is_auth {
                    platform.remove_node_auth(address);
                    auth_n -= 1;
                    alive_auth_n -= 1;
                    *is_auth = false;
                }
            });
            stopped_nodes.iter_mut().for_each(|(address, is_auth)| {
                if fortune_auth_n < auth_n && *is_auth {
                    platform.remove_node_auth(address);
                    auth_n -= 1;
                    *is_auth = false;
                }
            });
        }
        thread::sleep(Duration::from_secs(1));

        // do alive
        if fortune_alive_n > total_n {
            let address_list = platform.add_new_nodes(fortune_alive_n - total_n);
            total_n = fortune_alive_n;
            alive_nodes.append(
                &mut address_list
                    .into_iter()
                    .map(|address| (address, false))
                    .collect(),
            );
            while let Some((address, is_auth)) = stopped_nodes.pop() {
                platform.restart_node(&address);
                alive_n += 1;
                if is_auth {
                    alive_auth_n += 1;
                }
                alive_nodes.push((address, is_auth));
            }
        } else if fortune_alive_n > alive_n {
            let mut drain = false;
            while fortune_alive_n > alive_n && !drain {
                while let Some((address, is_auth)) = stopped_nodes.pop() {
                    platform.restart_node(&address);
                    alive_n += 1;
                    if is_auth {
                        alive_auth_n += 1;
                    }
                    alive_nodes.push((address, is_auth));
                }
                drain = true;
            }
        } else {
            while fortune_alive_n < alive_n && alive_auth_n > auth_n * 2 / 3 + 2 {
                let (address, is_auth) = alive_nodes.pop().expect("pop alive_nodes");
                platform.stop_node(&address);
                alive_n -= 1;
                if is_auth {
                    alive_auth_n -= 1;
                }
                stopped_nodes.push((address, is_auth));
            }
        }

        // ensure enough alive_node
        while alive_auth_n < auth_n * 2 / 3 + 1 {
            let (address, is_auth) = stopped_nodes.pop().expect("pop alive_nodes");
            platform.restart_node(&address);
            alive_n += 1;
            if is_auth {
                alive_auth_n += 1;
            }
            alive_nodes.push((address, is_auth));
        }

        println!("[CASE {}] end\n\t<state> total_n: {}, alive_n: {}\n\t<auth> auth_n: {}, alive_auth_n: {}, auth_list: {:?}\n\t<alive> {:?}\n\t<stopped> {:?}\n\t<fortune> auth_n: {}, alive_n: {}\n",
                 cnt, total_n, alive_n, auth_n, alive_auth_n, platform.get_auth_list(),
                 alive_nodes.iter().map(|(address, bool)| (address.tiny_hex(), *bool)).collect::<Vec<(String, bool)>>(),
                 stopped_nodes.iter().map(|(address, bool)| (address.tiny_hex(), *bool)).collect::<Vec<(String, bool)>>(),
                 fortune_auth_n, fortune_alive_n);
        thread::sleep(Duration::from_secs(10));
        cnt += 1;
    }
}

// do auth first
// auth_n:
//          if auth_n > total_n, add nodes, then auth them;
//          else if auth_n > auth_n, first iterate alive_nodes to auth unauth nodes, if not enough,
// iterate stopped_nodes;          else first iterate alive_nodes to unauth auth nodes, if not
// enough, iterate stopped_nodes; alive_n:
//          if total_n < alive_n, add nodes; else stop node from alive_nodes, until finish or touch
// 2/3+ alive auth limit"
fn fortune_oracle() -> (usize, usize) {
    // auth_n and alive_n
    let auth_n = rand::thread_rng().gen::<usize>() % (NODE_LIMIT - 1) + 1;
    let min_alive_n = auth_n * 2 / 3 + 1;
    let alive_n = rand::thread_rng().gen::<usize>() % (NODE_LIMIT - min_alive_n) + min_alive_n;
    (auth_n, alive_n)
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
