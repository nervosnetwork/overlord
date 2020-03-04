use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use bytes::Bytes;
use creep::Context;
use crossbeam_channel::{unbounded, Receiver, Sender};

use overlord::types::{Node, OverlordMsg, Status};

use super::primitive::{Block, Channel, Participant};
use super::utils::{get_index, get_index_array, get_max_alive_height, timer_config};
use super::wal::{Record, RECORD_TMP_FILE};

pub async fn run_test(records: Record, refresh_height: u64, test_height: u64) {
    let records = Arc::new(records);
    let interval = records.interval;
    let start_height = get_max_alive_height(&records, &records.node_record);
    println!("Test start with {:?} nodes，interval of {:?} ms, refresh every {:?} height, begin with {:?} height and terminate after {:?} height",
             records.node_record.len(), interval, refresh_height, start_height, test_height);

    let mut test_count = 0;
    let mut alive_nodes = { records.alive_record.lock().unwrap().clone() };
    loop {
        println!(
            "Cycle {:?} start, generate {:?} alive_nodes of {:?}",
            test_count,
            alive_nodes.len(),
            get_index_array(&records.node_record, &alive_nodes)
        );

        let height_start = get_max_alive_height(&records, &alive_nodes);

        let alive_handlers = run_alive_nodes(&records, alive_nodes.clone());
        synchronize_height(
            &records,
            alive_nodes.clone(),
            alive_handlers.clone(),
            test_count,
        );

        let mut height_end = get_max_alive_height(&records, &alive_nodes);
        let mut last_max_height = height_end;
        let mut stagnation = 0;
        while height_end - height_start < refresh_height {
            thread::sleep(Duration::from_millis(interval));
            height_end = get_max_alive_height(&records, &alive_nodes);
            if height_end == last_max_height {
                stagnation += 1;
            } else {
                stagnation = 0;
                last_max_height = height_end;
            }
            if stagnation > 2000 / interval {
                println!("consensus stagnation time exceeded {:?} s, save wal", 2);
                records.save(RECORD_TMP_FILE);
                stagnation = 0;
            }
        }
        println!(
            "Cycle {:?} start from {:?}, end with {:?}",
            test_count, height_start, height_end
        );

        kill_alive_nodes(alive_handlers);

        test_count += 1;

        if height_end - start_height > test_height {
            break;
        }

        alive_nodes = records.update_alive();
    }
}

fn run_alive_nodes(records: &Arc<Record>, alive_nodes: Vec<Node>) -> Vec<Arc<Participant>> {
    let interval = records.interval;
    let alive_num = alive_nodes.len();

    let channels: Vec<Channel> = (0..alive_num).map(|_| unbounded()).collect();
    let hearings: HashMap<Bytes, Receiver<OverlordMsg<Block>>> = alive_nodes
        .iter()
        .map(|node| node.address.clone())
        .zip(channels.iter().map(|(_, receiver)| receiver.clone()))
        .collect();

    let mut alive_handlers = Vec::new();
    for node in alive_nodes.iter() {
        let name = node.address.clone();
        let mut talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>> = alive_nodes
            .iter()
            .map(|node| node.address.clone())
            .zip(channels.iter().map(|(sender, _)| sender.clone()))
            .collect();
        talk_to.remove(&name);

        let node = Arc::new(Participant::new(
            &name,
            talk_to,
            hearings.get(&name).unwrap().clone(),
            records,
        ));

        alive_handlers.push(Arc::<Participant>::clone(&node));

        let list = records.node_record.clone();
        tokio::spawn(async move {
            node.run(interval, timer_config(), list).await.unwrap();
        });
    }
    alive_handlers
}

fn synchronize_height(
    records: &Arc<Record>,
    alive_nodes: Vec<Node>,
    alive_handlers: Vec<Arc<Participant>>,
    test_count: u64,
) {
    let records = Arc::<Record>::clone(records);

    tokio::spawn(async move {
        thread::sleep(Duration::from_millis(records.interval));
        let max_height = get_max_alive_height(&records, &alive_nodes);
        let height_record = records.height_record.lock().unwrap();
        height_record.iter().for_each(|(name, height)| {
            if *height < max_height {
                alive_handlers
                    .iter()
                    .filter(|node| node.adapter.name == name)
                    .for_each(|node| {
                        println!(
                            "Cycle {:?}, synchronize {:?} to node {:?} of height {:?}",
                            test_count,
                            max_height + 1,
                            get_index(&records.node_record, name),
                            height
                        );
                        let _ = node.handler.send_msg(
                            Context::new(),
                            OverlordMsg::RichStatus(Status {
                                height:         max_height + 1,
                                interval:       Some(records.interval),
                                timer_config:   timer_config(),
                                authority_list: records.node_record.clone(),
                            }),
                        );
                    });
            }
        });
    });
}

fn kill_alive_nodes(alive_handlers: Vec<Arc<Participant>>) {
    alive_handlers.iter().for_each(|node| {
        node.stopped.store(true, Ordering::Relaxed);
        node.handler
            .send_msg(Context::new(), OverlordMsg::Stop)
            .unwrap()
    });
}
