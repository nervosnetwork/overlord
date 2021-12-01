use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bytes::Bytes;
use creep::Context;
use crossbeam_channel::{unbounded, Receiver, Sender};

use overlord::types::{Node, OverlordMsg, Status};

use super::primitive::{Block, Channel, Participant};
use super::utils::{get_max_alive_height, timer_config, to_hex, to_hex_strings};
use super::wal::{Record, RECORD_TMP_FILE};

pub async fn run_test(records: Record, refresh_height: u64, test_height: u64) {
    let interval = records.interval;
    let start_height = get_max_alive_height(&records.height_record, &records.node_record);
    println!("Test start with {:?} nodes，interval of {:?} ms, refresh every {:?} height, begin with {:?} height and terminate after {:?} height",
             records.node_record.len(), interval, refresh_height, start_height, test_height);

    let mut test_id = 0;
    let mut alive_nodes = { records.alive_record.lock().unwrap().clone() };
    loop {
        println!(
            "Cycle {:?} start, generate {:?} alive_nodes of {:?}",
            test_id,
            alive_nodes.len(),
            to_hex_strings(&alive_nodes)
        );

        let height_start = get_max_alive_height(&records.height_record, &alive_nodes);

        let (alive_handlers, senders) = run_alive_nodes(&records, alive_nodes.clone());
        synchronize_height(
            &records,
            alive_nodes.clone(),
            alive_handlers.clone(),
            test_id,
        );

        let mut height_end = get_max_alive_height(&records.height_record, &alive_nodes);
        let mut last_max_height = height_end;
        let mut stagnation = 0;
        while height_end - height_start < refresh_height {
            thread::sleep(Duration::from_millis(interval));
            height_end = get_max_alive_height(&records.height_record, &alive_nodes);
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
            test_id, height_start, height_end
        );

        kill_alive_nodes(alive_handlers, senders);

        test_id += 1;

        if height_end - start_height > test_height {
            break;
        }

        alive_nodes = records.update_alives(test_id);
    }
}

fn run_alive_nodes(
    records: &Record,
    alive_nodes: Vec<Node>,
) -> (Vec<Arc<Participant>>, Vec<Sender<OverlordMsg<Block>>>) {
    let records = records.as_internal();
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
        let address = node.address.clone();
        let mut talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>> = alive_nodes
            .iter()
            .map(|node| node.address.clone())
            .zip(channels.iter().map(|(sender, _)| sender.clone()))
            .collect();
        talk_to.remove(&address);

        let node = Arc::new(Participant::new(
            &address,
            talk_to,
            hearings.get(&address).unwrap().clone(),
            records.clone(),
        ));

        alive_handlers.push(Arc::<Participant>::clone(&node));

        let list = records.node_record.clone();
        tokio::spawn(async move {
            node.run(interval, timer_config(), list).await.unwrap();
        });
    }
    (
        alive_handlers,
        channels.iter().map(|channel| channel.0.clone()).collect(),
    )
}

fn synchronize_height(
    records: &Record,
    alive_nodes: Vec<Node>,
    alive_handlers: Vec<Arc<Participant>>,
    test_id: u64,
) {
    let interval = records.interval;
    let height_record = Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&records.height_record);
    let node_record = records.node_record.clone();

    tokio::spawn(async move {
        thread::sleep(Duration::from_millis(interval));
        let max_height = get_max_alive_height(&height_record, &alive_nodes);
        let height_record = height_record.lock().unwrap();
        height_record.iter().for_each(|(address, height)| {
            if *height < max_height {
                alive_handlers
                    .iter()
                    .filter(|node| node.adapter.address == address)
                    .for_each(|node| {
                        println!(
                            "Cycle {:?}, synchronize {:?} to node {:?} of height {:?}",
                            test_id,
                            max_height + 1,
                            to_hex(address),
                            height
                        );
                        let _ = node.handler.send_msg(
                            Context::new(),
                            OverlordMsg::RichStatus(Status {
                                height: max_height + 1,
                                interval: Some(interval),
                                timer_config: timer_config(),
                                authority_list: node_record.clone(),
                            }),
                        );
                    });
            }
        });
    });
}

fn kill_alive_nodes(
    alive_handlers: Vec<Arc<Participant>>,
    senders: Vec<Sender<OverlordMsg<Block>>>,
) {
    alive_handlers.iter().for_each(|node| {
        node.handler
            .send_msg(Context::new(), OverlordMsg::Stop)
            .unwrap()
    });
    senders
        .iter()
        .for_each(|sender| sender.send(OverlordMsg::Stop).unwrap());
}
