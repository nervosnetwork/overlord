use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use lru_cache::LruCache;
use rlp::Encodable;
use serde::{Deserialize, Serialize};
use serde_json;

use overlord::types::Node;
use overlord::{Wal, WalInfo};

use super::primitive::Block;
use super::utils::{create_alive_nodes, gen_random_bytes};
use crate::integration_tests::utils::to_hex;

pub const RECORD_TMP_FILE: &str = "./tests/integration_tests/test.json";

#[derive(Clone)]
pub struct MockWal {
    test_id:         u64,
    test_id_updated: Arc<Mutex<u64>>,
    address:         Bytes,
    content:         Arc<Mutex<Option<Bytes>>>,
}

impl MockWal {
    pub fn new(
        test_id_updated: &Arc<Mutex<u64>>,
        addr: Bytes,
        content: &Arc<Mutex<Option<Bytes>>>,
    ) -> MockWal {
        MockWal {
            test_id:         *test_id_updated.lock().unwrap(),
            address:         addr,
            test_id_updated: Arc::<Mutex<u64>>::clone(test_id_updated),
            content:         Arc::<Mutex<Option<Bytes>>>::clone(content),
        }
    }
}

#[async_trait]
impl Wal for MockWal {
    async fn save(&self, info: Bytes) -> Result<(), Box<dyn Error + Send>> {
        let test_id_updated = *self.test_id_updated.lock().unwrap();
        // avoid previous test overwrite wal of the latest test
        if test_id_updated == self.test_id {
            // let content: WalInfo<Block> = rlp::decode(&info).unwrap();
            // println!("{:?} save {:?}", to_hex(&self.address), content);
            *self.content.lock().unwrap() = Some(info);
        } else {
            println!("previous test try to overwrite wal");
        }
        Ok(())
    }

    async fn load(&self) -> Result<Option<Bytes>, Box<dyn Error + Send>> {
        let info = self.content.lock().unwrap().as_ref().cloned();
        if let Some(info) = info.clone() {
            let content: WalInfo<Block> = rlp::decode(&info).unwrap();
            println!("{:?} load {:?}", to_hex(&self.address), content);
        }
        Ok(info)
    }
}

pub struct Record {
    pub test_id:       Arc<Mutex<u64>>,
    pub node_record:   Vec<Node>,
    pub alive_record:  Mutex<Vec<Node>>,
    pub wal_record:    HashMap<Bytes, MockWal>,
    pub commit_record: Arc<Mutex<LruCache<u64, Bytes>>>,
    pub height_record: Arc<Mutex<HashMap<Bytes, u64>>>,
    pub interval:      u64,
}

impl Record {
    pub fn new(num: usize, interval: u64) -> Record {
        let test_id = Arc::new(Mutex::new(0));
        let node_record: Vec<Node> = (0..num).map(|_| Node::new(gen_random_bytes())).collect();
        let alive_record = Mutex::new(create_alive_nodes(node_record.clone()));
        let wal_record: HashMap<Bytes, MockWal> = (0..num)
            .map(|i| {
                let address = node_record.get(i).unwrap().address.clone();
                (
                    address.clone(),
                    MockWal::new(
                        &Arc::new(Mutex::new(0)),
                        address,
                        &Arc::new(Mutex::new(None)),
                    ),
                )
            })
            .collect();
        let commit_record: Arc<Mutex<LruCache<u64, Bytes>>> =
            Arc::new(Mutex::new(LruCache::new(10)));
        commit_record.lock().unwrap().insert(0, gen_random_bytes());
        let height_record: Arc<Mutex<HashMap<Bytes, u64>>> = Arc::new(Mutex::new(
            (0..num)
                .map(|i| (node_record.get(i).unwrap().address.clone(), 0))
                .collect(),
        ));

        Record {
            test_id,
            node_record,
            alive_record,
            wal_record,
            commit_record,
            height_record,
            interval,
        }
    }

    fn to_wal(&self) -> RecordForWal {
        let test_id = *self.test_id.lock().unwrap();
        let node_record = self.node_record.clone();
        let alive_record = self.alive_record.lock().unwrap().clone();
        let wal_record: Vec<TupleWalRecord> = self
            .wal_record
            .iter()
            .map(|(address, wal)| {
                TupleWalRecord(
                    address.clone(),
                    wal.content
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|wal| rlp::decode(&wal).unwrap()),
                )
            })
            .collect();
        let commit_record: Vec<TupleCommitRecord> = self
            .commit_record
            .lock()
            .unwrap()
            .iter()
            .map(|(height, commit_hash)| TupleCommitRecord(*height, commit_hash.clone()))
            .collect();
        let height_record: Vec<TupleHeightRecord> = self
            .height_record
            .lock()
            .unwrap()
            .iter()
            .map(|(address, height)| TupleHeightRecord(address.clone(), *height))
            .collect();
        let interval = self.interval;
        RecordForWal {
            test_id,
            node_record,
            alive_record,
            wal_record,
            commit_record,
            height_record,
            interval,
        }
    }

    pub fn update_alives(&self, test_id: u64) -> Vec<Node> {
        self.update_alive_records();
        self.update_test_id(test_id);
        self.alive_record.lock().unwrap().clone()
    }

    fn update_test_id(&self, new_test_id: u64) {
        *self.test_id.lock().unwrap() = new_test_id;
    }

    fn update_alive_records(&self) {
        let alive_record = create_alive_nodes(self.node_record.clone());
        *self.alive_record.lock().unwrap() = alive_record;
    }

    pub fn as_internal(&self) -> RecordInternal {
        let test_id = *self.test_id.lock().unwrap();
        let test_id_updated = Arc::<Mutex<u64>>::clone(&self.test_id);
        let node_record = self.node_record.clone();
        let alive_record = self.alive_record.lock().unwrap().clone();
        let wal_record: HashMap<Bytes, MockWal> = self
            .wal_record
            .iter()
            .map(|(address, wal)| {
                (
                    address.clone(),
                    MockWal::new(&test_id_updated, address.clone(), &wal.content),
                )
            })
            .collect();
        let commit_record = Arc::<Mutex<LruCache<u64, Bytes>>>::clone(&self.commit_record);
        let height_record = Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&self.height_record);
        let interval = self.interval;

        RecordInternal {
            test_id,
            test_id_updated,
            node_record,
            alive_record,
            wal_record,
            commit_record,
            height_record,
            interval,
        }
    }

    pub fn save(&self, filename: &str) {
        let file = File::create(filename).unwrap();
        let record_for_wal = self.to_wal();
        serde_json::to_writer_pretty(file, &record_for_wal).unwrap();
    }

    pub fn load(filename: &str) -> Record {
        let file = File::open(filename).unwrap();
        let reader = BufReader::new(file);
        let record_for_wal: RecordForWal = serde_json::from_reader(reader).unwrap();
        record_for_wal.to_record()
    }
}

#[derive(Clone)]
pub struct RecordInternal {
    pub test_id:         u64,
    pub test_id_updated: Arc<Mutex<u64>>,
    pub node_record:     Vec<Node>,
    pub alive_record:    Vec<Node>,
    pub wal_record:      HashMap<Bytes, MockWal>,
    pub commit_record:   Arc<Mutex<LruCache<u64, Bytes>>>,
    pub height_record:   Arc<Mutex<HashMap<Bytes, u64>>>,
    pub interval:        u64,
}

impl RecordInternal {
    fn to_wal(&self) -> RecordForWal {
        let test_id = self.test_id;
        let node_record = self.node_record.clone();
        let alive_record = self.alive_record.clone();
        let wal_record: Vec<TupleWalRecord> = self
            .wal_record
            .iter()
            .map(|(address, wal)| {
                TupleWalRecord(
                    address.clone(),
                    wal.content
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|wal| rlp::decode(&wal).unwrap()),
                )
            })
            .collect();
        let commit_record: Vec<TupleCommitRecord> = self
            .commit_record
            .lock()
            .unwrap()
            .iter()
            .map(|(height, commit_hash)| TupleCommitRecord(*height, commit_hash.clone()))
            .collect();
        let height_record: Vec<TupleHeightRecord> = self
            .height_record
            .lock()
            .unwrap()
            .iter()
            .map(|(address, height)| TupleHeightRecord(address.clone(), *height))
            .collect();
        let interval = self.interval;
        RecordForWal {
            test_id,
            node_record,
            alive_record,
            wal_record,
            commit_record,
            height_record,
            interval,
        }
    }

    pub fn save(&self, filename: &str) {
        let file = File::create(filename).unwrap();
        let record_for_wal = self.to_wal();
        serde_json::to_writer_pretty(file, &record_for_wal).unwrap();
    }
}

#[derive(Serialize, Deserialize)]
struct TupleWalRecord(
    #[serde(with = "overlord::serde_hex")] Bytes,
    Option<WalInfo<Block>>,
);

#[derive(Serialize, Deserialize, Clone)]
struct TupleCommitRecord(u64, #[serde(with = "overlord::serde_hex")] Bytes);

#[derive(Serialize, Deserialize, Clone)]
struct TupleHeightRecord(#[serde(with = "overlord::serde_hex")] Bytes, u64);

#[derive(Serialize, Deserialize)]
struct RecordForWal {
    test_id:       u64,
    node_record:   Vec<Node>,
    alive_record:  Vec<Node>,
    wal_record:    Vec<TupleWalRecord>,
    commit_record: Vec<TupleCommitRecord>,
    height_record: Vec<TupleHeightRecord>,
    interval:      u64,
}

impl RecordForWal {
    fn to_record(&self) -> Record {
        let test_id = Arc::new(Mutex::new(self.test_id));
        let node_record = self.node_record.clone();
        let alive_record = Mutex::new(self.alive_record.clone());
        let wal_record: HashMap<Bytes, MockWal> = self
            .wal_record
            .iter()
            .map(|TupleWalRecord(address, wal)| {
                (address.clone(), MockWal {
                    test_id:         self.test_id,
                    test_id_updated: Arc::clone(&test_id),
                    address:         address.clone(),
                    content:         Arc::new(Mutex::new(
                        wal.as_ref().map(|wal| Bytes::from(wal.rlp_bytes())),
                    )),
                })
            })
            .collect();
        let mut commit_record: LruCache<u64, Bytes> = LruCache::new(10);
        for TupleCommitRecord(height, commit_hash) in self.commit_record.clone() {
            commit_record.insert(height, commit_hash);
        }
        let height_record: HashMap<Bytes, u64> = self
            .height_record
            .iter()
            .map(|TupleHeightRecord(address, height)| (address.clone(), *height))
            .collect();
        Record {
            test_id,
            node_record,
            alive_record,
            wal_record,
            commit_record: Arc::new(Mutex::new(commit_record)),
            height_record: Arc::new(Mutex::new(height_record)),
            interval: self.interval,
        }
    }
}
