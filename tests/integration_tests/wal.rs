use std::error::Error;
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;

use overlord::types::Node;
use overlord::Wal;

pub struct MockWal {
    name:     Bytes,
    speakers: Vec<Node>,
    inner:    Mutex<Option<Bytes>>,
}

impl MockWal {
    pub fn new(name: Bytes, speakers: Vec<Node>) -> MockWal {
        MockWal {
            name,
            speakers,
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
