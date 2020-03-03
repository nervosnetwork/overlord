use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bytes::Bytes;
use creep::Context;
use crossbeam_channel::{unbounded, Receiver, Sender};
use lru_cache::LruCache;

use overlord::types::{Node, OverlordMsg, Status};

use super::primitive::{Block, Channel, Participant};
use super::utils::{
    gen_alive_nodes, gen_random_bytes, get_index, get_index_array, get_max_alive_height,
    timer_config,
};
use super::wal::MockWal;

pub async fn run_test(num: usize, interval: u64, change_nodes_cycles: u64, test_height: u64) {
    let nodes: Vec<Node> = (0..num).map(|_| Node::new(gen_random_bytes())).collect();
    println!("Wal test start, generate {:?} nodes", num);
    let wal_record: HashMap<Bytes, Arc<MockWal>> = (0..num)
        .map(|i| {
            (
                nodes.get(i).unwrap().address.clone(),
                Arc::new(MockWal::new(
                    nodes.get(i).unwrap().address.clone(),
                    nodes.clone(),
                )),
            )
        })
        .collect();
    let commit_record: Arc<Mutex<LruCache<u64, Bytes>>> = Arc::new(Mutex::new(LruCache::new(10)));
    let height_record: Arc<Mutex<HashMap<Bytes, u64>>> = Arc::new(Mutex::new(HashMap::new()));

    let mut test_count = 0;
    loop {
        let alive_nodes = if test_count == 0 {
            nodes.clone()
        } else {
            gen_alive_nodes(nodes.clone())
        };
        let alive_num = alive_nodes.len();
        println!(
            "Cycle {:?} start, generate {:?} alive_nodes of {:?}",
            test_count,
            alive_num,
            get_index_array(&nodes, &alive_nodes)
        );

        let height_start = get_max_alive_height(
            Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record),
            &alive_nodes,
        );

        let channels: Vec<Channel> = (0..alive_num).map(|_| unbounded()).collect();
        let hearings: HashMap<Bytes, Receiver<OverlordMsg<Block>>> = alive_nodes
            .iter()
            .map(|node| node.address.clone())
            .zip(channels.iter().map(|(_, receiver)| receiver.clone()))
            .collect();

        let alive_nodes_clone = alive_nodes.clone();
        let auth_list = nodes.clone();
        let mut handlers = Vec::new();
        for node in alive_nodes.iter() {
            let name = node.address.clone();
            let mut talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>> = alive_nodes_clone
                .iter()
                .map(|node| node.address.clone())
                .zip(channels.iter().map(|(sender, _)| sender.clone()))
                .collect();
            talk_to.remove(&name);

            let node = Arc::new(Participant::new(
                (name.clone(), interval),
                auth_list.clone(),
                talk_to,
                hearings.get(&name).unwrap().clone(),
                Arc::<Mutex<LruCache<u64, Bytes>>>::clone(&commit_record),
                Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record),
                Arc::<MockWal>::clone(wal_record.get(&name).unwrap()),
            ));

            handlers.push(Arc::<Participant>::clone(&node));

            let list = auth_list.clone();
            tokio::spawn(async move {
                node.run(interval, timer_config(), list).await.unwrap();
            });
        }

        // synchronization height
        let height_record_clone = Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record);
        let handles_clone = handlers.clone();
        let nodes_clone = nodes.clone();
        let alive_nodes_clone = alive_nodes.clone();
        tokio::spawn(async move {
            thread::sleep(Duration::from_millis(interval));
            let max_height = get_max_alive_height(
                Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record_clone),
                &alive_nodes_clone,
            );
            {
                let height_record = height_record_clone.lock().unwrap();
                height_record.iter().for_each(|(name, height)| {
                    // println!("node {:?} in height {:?}", get_index(&nodes_clone, name),
                    // height);
                    if *height < max_height {
                        handles_clone
                            .iter()
                            .filter(|node| node.adapter.name == name)
                            .for_each(|node| {
                                println!(
                                    "Cycle {:?}, synchronize {:?} to node {:?} of height {:?}",
                                    test_count,
                                    max_height + 1,
                                    get_index(&nodes_clone, name),
                                    height
                                );
                                let _ = node.handler.send_msg(
                                    Context::new(),
                                    OverlordMsg::RichStatus(Status {
                                        height:         max_height + 1,
                                        interval:       Some(interval),
                                        timer_config:   timer_config(),
                                        authority_list: nodes_clone.clone(),
                                    }),
                                );
                            });
                    }
                });
            }
        });

        let mut height_end = get_max_alive_height(
            Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record),
            &alive_nodes,
        );
        while height_end - height_start < change_nodes_cycles {
            thread::sleep(Duration::from_millis(interval));
            height_end = get_max_alive_height(
                Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record),
                &alive_nodes,
            );
        }
        println!(
            "Cycle {:?} start from {:?}, end with {:?}",
            test_count, height_start, height_end
        );

        // close consensus process
        println!("Cycle {:?} end, kill alive-nodes", test_count);
        handlers.iter().for_each(|node| {
            node.stopped.store(true, Ordering::Relaxed);
            node.handler
                .send_msg(Context::new(), OverlordMsg::Stop)
                .unwrap()
        });

        test_count += 1;

        if height_end > test_height {
            break;
        }
    }
}
