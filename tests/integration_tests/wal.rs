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
use super::utils::gen_random_bytes;

pub const RECORD_TMP_FILE: &str = "./tests/integration_tests/test_case/case0.json";

pub struct MockWal {
    inner: Mutex<Option<Bytes>>,
}

impl MockWal {
    pub fn new() -> MockWal {
        MockWal {
            inner: Mutex::new(None),
        }
    }
}

#[async_trait]
impl Wal for MockWal {
    async fn save(&self, info: Bytes) -> Result<(), Box<dyn Error + Send>> {
        *self.inner.lock().unwrap() = Some(info);
        Ok(())
    }

    async fn load(&self) -> Result<Option<Bytes>, Box<dyn Error + Send>> {
        Ok(self.inner.lock().unwrap().as_ref().cloned())
    }
}

pub struct Record {
    pub node_record:   Vec<Node>,
    pub wal_record:    HashMap<Bytes, Arc<MockWal>>,
    pub commit_record: Arc<Mutex<LruCache<u64, Bytes>>>,
    pub height_record: Arc<Mutex<HashMap<Bytes, u64>>>,
    pub interval:      u64,
}

impl Record {
    pub fn new(num: usize, interval: u64) -> Record {
        let node_record: Vec<Node> = (0..num).map(|_| Node::new(gen_random_bytes())).collect();
        let wal_record: HashMap<Bytes, Arc<MockWal>> = (0..num)
            .map(|i| {
                (
                    node_record.get(i).unwrap().address.clone(),
                    Arc::new(MockWal::new()),
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
            node_record,
            wal_record,
            commit_record,
            height_record,
            interval,
        }
    }

    fn into_wal(&self) -> RecordForWal {
        let node_record = self.node_record.clone();
        let wal_record: Vec<(Bytes, Option<WalInfo<Block>>)> = self
            .wal_record
            .iter()
            .map(|(name, wal)| {
                (
                    name.clone(),
                    wal.inner
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|wal| rlp::decode(&wal).unwrap()),
                )
            })
            .collect();
        let commit_record: Vec<(u64, Bytes)> = self
            .commit_record
            .lock()
            .unwrap()
            .iter()
            .map(|(height, commit_hash)| (*height, commit_hash.clone()))
            .collect();
        let height_record: Vec<(Bytes, u64)> = self
            .height_record
            .lock()
            .unwrap()
            .iter()
            .map(|(name, height)| (name.clone(), *height))
            .collect();
        let interval = self.interval;
        RecordForWal {
            node_record,
            wal_record,
            commit_record,
            height_record,
            interval,
        }
    }

    pub fn save(&self, filename: &str) {
        let file = File::create(filename).unwrap();
        let record_for_wal = self.into_wal();
        serde_json::to_writer_pretty(file, &record_for_wal).unwrap();
    }

    pub fn load(filename: &str) -> Record {
        let file = File::open(filename).unwrap();
        let reader = BufReader::new(file);
        let record_for_wal: RecordForWal = serde_json::from_reader(reader).unwrap();
        record_for_wal.into_record()
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct RecordForWal {
    node_record:   Vec<Node>,
    wal_record:    Vec<(Bytes, Option<WalInfo<Block>>)>,
    commit_record: Vec<(u64, Bytes)>,
    height_record: Vec<(Bytes, u64)>,
    interval:      u64,
}

impl RecordForWal {
    fn into_record(&self) -> Record {
        let node_record = self.node_record.clone();
        let wal_record: HashMap<Bytes, Arc<MockWal>> = self
            .wal_record
            .iter()
            .map(|(name, wal)| {
                (
                    name.clone(),
                    Arc::new(MockWal {
                        inner: Mutex::new(wal.as_ref().map(|wal| Bytes::from(wal.rlp_bytes()))),
                    }),
                )
            })
            .collect();
        let mut commit_record: LruCache<u64, Bytes> = LruCache::new(10);
        for (height, commit_hash) in self.commit_record.clone() {
            commit_record.insert(height, commit_hash);
        }
        let height_record: HashMap<Bytes, u64> = self
            .height_record
            .iter()
            .map(|(name, height)| (name.clone(), *height))
            .collect();
        Record {
            node_record,
            wal_record,
            commit_record: Arc::new(Mutex::new(commit_record)),
            height_record: Arc::new(Mutex::new(height_record)),
            interval: self.interval,
        }
    }
}

#[test]
fn test_record() {
    let record_0 = Record::new(10, 1000);
    record_0
        .commit_record
        .lock()
        .unwrap()
        .insert(99, gen_random_bytes());
    record_0
        .height_record
        .lock()
        .unwrap()
        .insert(gen_random_bytes(), 100);
    record_0.save(RECORD_TMP_FILE);
    let _record_1 = Record::load(RECORD_TMP_FILE);
}
