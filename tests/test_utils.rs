use std::error::Error;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use blake2b_simd::blake2b;
use bytes::{Bytes, BytesMut};
use creep::Context;
use crossbeam_channel::Sender;
use overlord::error::ConsensusError;
use overlord::types::{Address, Commit, Hash, Node, OverlordMsg, Signature, Status};
use overlord::{Codec, Consensus, Crypto};
use rand::random;
use serde::{Deserialize, Serialize};

enum Approach {
    Broadcast,
    Directly(Address),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct Pill {
    height: u64,
    epoch:  Vec<u64>,
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
    fn new(height: u64) -> Self {
        let epoch = (0..128).map(|_| random::<u64>()).collect::<Vec<_>>();
        Pill { height, epoch }
    }
}

struct ConsensusHelper<T: Codec> {
    msg_tx:    Sender<Msg<T>>,
    commit_tx: Sender<Commit<T>>,
    auth_list: Vec<Node>,
}

#[async_trait]
impl Consensus<Pill> for ConsensusHelper<Pill> {
    async fn get_block(
        &self,
        _ctx: Context,
        height: u64,
    ) -> Result<(Pill, Hash), Box<dyn Error + Send>> {
        let epoch = Pill::new(height);
        let hash = BytesMut::from(blake2b(epoch.clone().encode()?.as_ref()).as_bytes()).freeze();
        Ok((epoch, hash))
    }

    async fn check_block(
        &self,
        _ctx: Context,
        _height: u64,
        _hash: Hash,
        _epoch: Pill,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }

    async fn commit(
        &self,
        _ctx: Context,
        height: u64,
        commit: Commit<Pill>,
    ) -> Result<Status, Box<dyn Error + Send>> {
        self.commit_tx.send(commit).unwrap();
        let status = Status {
            height:         height + 1,
            interval:       None,
            timer_config:   None,
            authority_list: self.auth_list.clone(),
        };
        Ok(status)
    }

    async fn get_authority_list(
        &self,
        _ctx: Context,
        _height: u64,
    ) -> Result<Vec<Node>, Box<dyn Error + Send>> {
        Ok(self.auth_list.clone())
    }

    async fn broadcast_to_other(
        &self,
        _ctx: Context,
        msg: OverlordMsg<Pill>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let message = Msg {
            content:  msg,
            approach: Approach::Broadcast,
        };

        self.msg_tx.send(message).unwrap();
        Ok(())
    }

    async fn transmit_to_relayer(
        &self,
        _ctx: Context,
        addr: Address,
        msg: OverlordMsg<Pill>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let message = Msg {
            content:  msg,
            approach: Approach::Directly(addr),
        };

        self.msg_tx.send(message).unwrap();
        Ok(())
    }

    fn report_error(&self, _ctx: Context, _err: ConsensusError) {}
}

#[derive(Clone)]
struct BlsCrypto(Address);

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
        _voter: Address,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }

    fn aggregate_signatures(
        &self,
        _signatures: Vec<Signature>,
        _voters: Vec<Address>,
    ) -> Result<Signature, Box<dyn Error + Send>> {
        Ok(gen_hash())
    }

    fn verify_aggregated_signature(
        &self,
        _aggregate_signature: Signature,
        _hash: Hash,
        _voters: Vec<Address>,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }
}

// impl BlsCrypto {
//     fn new(addr: Address) -> Self {
//         BlsCrypto(addr)
//     }
// }

struct Msg<T: Codec> {
    pub content:  OverlordMsg<T>,
    pub approach: Approach,
}

fn gen_hash() -> Hash {
    Hash::from((0..16).map(|_| random::<u8>()).collect::<Vec<_>>())
}
