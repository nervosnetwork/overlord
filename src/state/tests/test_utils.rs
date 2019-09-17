use std::error::Error;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use bytes::Bytes;
use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

use crate::types::{
    Address, AggregatedSignature, Commit, Hash, Node, OverlordMsg, Signature, Status,
};
use crate::{Codec, Consensus, Crypto};

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
    msg_tx:    Sender<OverlordMsg<T>>,
    commit_tx: Sender<Commit<T>>,
    auth_list: Vec<Node>,
}

#[async_trait]
impl Consensus<Pill> for ConsensusHelper<Pill> {
    async fn get_epoch(
        &self,
        _ctx: Vec<u8>,
        epoch_id: u64,
    ) -> Result<(Pill, Hash), Box<dyn Error + Send>> {
        let epoch = Pill::new(epoch_id);
        let hash = Bytes::from(vec![1u8, 2, 3, 4, 5]);
        Ok((epoch, hash))
    }

    async fn check_epoch(
        &self,
        _ctx: Vec<u8>,
        epoch_id: u64,
        _hash: Hash,
    ) -> Result<Pill, Box<dyn Error + Send>> {
        Ok(Pill::new(epoch_id))
    }

    async fn commit(
        &self,
        _ctx: Vec<u8>,
        epoch_id: u64,
        commit: Commit<Pill>,
    ) -> Result<Status, Box<dyn Error + Send>> {
        self.commit_tx.send(commit).unwrap();
        let status = Status {
            epoch_id:       epoch_id + 1,
            interval:       None,
            authority_list: self.auth_list.clone(),
        };
        Ok(status)
    }

    async fn get_authority_list(
        &self,
        _ctx: Vec<u8>,
        _epoch_id: u64,
    ) -> Result<Vec<Node>, Box<dyn Error + Send>> {
        Ok(self.auth_list.clone())
    }

    async fn broadcast_to_other(
        &self,
        _ctx: Vec<u8>,
        msg: OverlordMsg<Pill>,
    ) -> Result<(), Box<dyn Error + Send>> {
        self.msg_tx.send(msg).unwrap();
        Ok(())
    }

    async fn transmit_to_relayer(
        &self,
        _ctx: Vec<u8>,
        _addr: Address,
        msg: OverlordMsg<Pill>,
    ) -> Result<(), Box<dyn Error + Send>> {
        self.msg_tx.send(msg).unwrap();
        Ok(())
    }
}

impl<T: Codec> ConsensusHelper<T> {
    pub fn new(msg_tx: Sender<OverlordMsg<T>>, commit_tx: Sender<Commit<T>>) -> Self {
        let auth_list = gen_auth_list();
        ConsensusHelper {
            msg_tx,
            commit_tx,
            auth_list,
        }
    }
}

#[derive(Clone)]
pub struct BlsCrypto(Address);

impl Crypto for BlsCrypto {
    fn hash(&self, _msg: Bytes) -> Hash {
        self.0.clone()
    }

    fn sign(&self, hash: Hash) -> Result<Signature, Box<dyn Error + Send>> {
        Ok(hash)
    }

    fn verify_signature(
        &self,
        _signature: Signature,
        _hash: Hash,
    ) -> Result<Address, Box<dyn Error + Send>> {
        Ok(self.0.clone())
    }

    fn aggregate_signatures(
        &self,
        _msgsignatures: Vec<Signature>,
    ) -> Result<Signature, Box<dyn Error + Send>> {
        use super::gen_hash;

        Ok(gen_hash())
    }

    fn verify_aggregated_signature(
        &self,
        _aggregate_signature: AggregatedSignature,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }
}

impl BlsCrypto {
    pub fn new(address: Address) -> Self {
        BlsCrypto(address)
    }
}

fn gen_auth_list() -> Vec<Node> {
    let tmp = vec![0, 1, 2, 3];
    tmp.iter()
        .map(|i| Node::new(Address::from(vec![*i as u8])))
        .collect::<Vec<_>>()
}
