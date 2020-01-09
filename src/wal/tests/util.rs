use std::error::Error;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use bytes::Bytes;
use creep::Context;
use futures::channel::mpsc::UnboundedSender;
use rand::random;
use serde::{Deserialize, Serialize};

use crate::types::{Address, AggregatedSignature, Commit, Hash, Node, OverlordMsg, Status};
use crate::wal::WalInfo;
use crate::{Codec, Consensus, Crypto, Wal};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Pill {
    epoch_id: u64,
    epoch:    Vec<u8>,
}

impl Codec for Pill {
    fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
        let encode: Vec<u8> = serialize(&self).expect("Serialize Pill error");
        Ok(Bytes::from(encode))
    }

    fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>> {
        let decode: Pill = deserialize(&data.as_ref()).expect("Deserialize Pill error.");
        Ok(decode)
    }
}

impl Pill {
    pub fn new(epoch_id: u64) -> Self {
        let epoch = vec![1, 2, 3, 4, 5];
        Pill { epoch_id, epoch }
    }
}

pub struct ConsensusHelper<T: Codec> {
    tx:        UnboundedSender<OverlordMsg<T>>,
    auth_list: Vec<Node>,
}

#[async_trait]
impl Consensus<Pill> for ConsensusHelper<Pill> {
    async fn get_epoch(
        &self,
        _ctx: Context,
        epoch_id: u64,
    ) -> Result<(Pill, Hash), Box<dyn Error + Send>> {
        let epoch = Pill::new(epoch_id);
        let hash = Bytes::from(vec![1u8, 2, 3, 4, 5]);
        Ok((epoch, hash))
    }

    async fn check_epoch(
        &self,
        _ctx: Context,
        _epoch_id: u64,
        _hash: Hash,
        _epoch: Pill,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }

    async fn commit(
        &self,
        _ctx: Context,
        epoch_id: u64,
        commit: Commit<Pill>,
    ) -> Result<Status, Box<dyn Error + Send>> {
        let _ = self.tx.unbounded_send(OverlordMsg::Commit(commit));
        let status = Status {
            epoch_id:       epoch_id + 1,
            interval:       None,
            authority_list: self.auth_list.clone(),
        };
        Ok(status)
    }

    async fn get_authority_list(
        &self,
        _ctx: Context,
        _epoch_id: u64,
    ) -> Result<Vec<Node>, Box<dyn Error + Send>> {
        Ok(self.auth_list.clone())
    }

    async fn broadcast_to_other(
        &self,
        _ctx: Context,
        msg: OverlordMsg<Pill>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let _ = self.tx.unbounded_send(msg);
        Ok(())
    }

    async fn transmit_to_relayer(
        &self,
        _ctx: Context,
        _addr: Address,
        msg: OverlordMsg<Pill>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let _ = self.tx.unbounded_send(msg);
        Ok(())
    }
}

impl<T: Codec> ConsensusHelper<T> {
    pub fn new(tx: UnboundedSender<OverlordMsg<T>>, auth_list: Vec<Node>) -> Self {
        ConsensusHelper { tx, auth_list }
    }
}

pub struct MockWal {
    inner: Bytes,
}

impl MockWal {
    pub fn new<T: Codec>(info: WalInfo<T>) -> Self {
        MockWal {
            inner: Bytes::from(rlp::encode(&info)),
        }
    }
}

#[async_trait]
impl Wal for MockWal {
    async fn save(&mut self, _info: Bytes) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }

    async fn load(&mut self) -> Result<Option<Bytes>, Box<dyn Error + Send>> {
        Ok(Some(self.inner.clone()))
    }
}

pub struct MockCrypto;

impl Crypto for MockCrypto {
    fn hash(&self, bytes: Bytes) -> Bytes {
        bytes
    }

    fn sign(&self, hash: Bytes) -> Result<Bytes, Box<dyn Error + Send>> {
        Ok(hash)
    }

    fn aggregate_signatures(
        &self,
        _signatures: Vec<Bytes>,
        _voters: Vec<Bytes>,
    ) -> Result<Bytes, Box<dyn Error + Send>> {
        Ok(Bytes::new())
    }

    fn verify_signature(
        &self,
        _signature: Bytes,
        _hash: Bytes,
        _voter: Bytes,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }

    fn verify_aggregated_signature(
        &self,
        _aggregated_signature: Bytes,
        _hash: Bytes,
        _voters: Vec<Bytes>,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }
}

pub fn gen_auth_list(nodes: usize) -> Vec<Node> {
    let mut res = vec![];
    for _i in 0..nodes {
        res.push(Node::new(Address::from(vec![random::<u8>()])));
    }
    res.sort();
    res
}

pub fn mock_aggregate_signature() -> AggregatedSignature {
    AggregatedSignature {
        signature:      Bytes::from((0..16).map(|_| random::<u8>()).collect::<Vec<_>>()),
        address_bitmap: Bytes::from((0..2).map(|_| random::<u8>()).collect::<Vec<_>>()),
    }
}
