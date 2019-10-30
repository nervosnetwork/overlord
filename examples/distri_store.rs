use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use creep::Context;
use crossbeam_channel::{unbounded, Receiver, Sender};
use hasher::{Hasher, HasherKeccak};
use lazy_static::lazy_static;
use rand::random;
use serde::{Deserialize, Serialize};

use overlord::types::{AggregatedSignature, Commit, Hash, Node, OverlordMsg, Status};
use overlord::{Codec, Consensus, Crypto, DurationConfig, Overlord, OverlordHandler};

lazy_static! {
    static ref HASHER_INST: HasherKeccak = HasherKeccak::new();
}

const NODE_NUM: u8 = 20;

const INTERVAL: u64 = 1000;

type Chan = (Sender<OverlordMsg<Message>>, Receiver<OverlordMsg<Message>>);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Message {
    inner: Bytes,
}

impl Message {
    fn from(bytes: Bytes) -> Self {
        Message { inner: bytes }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Detail {
    inner: Bytes,
}

impl Detail {
    fn from(msg: Message) -> Self {
        Detail {
            // TODO: expand your data
            inner: msg.inner,
        }
    }
}

macro_rules! impl_codec_for {
    ($($struc: ident),+) => {
        $(
            impl Codec for $struc {
                fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
                    Ok(Bytes::from(bincode::serialize(&self.inner).unwrap()))
                }

                fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>> {
                    let data: Option<Bytes> = bincode::deserialize(&data).unwrap();
                    Ok($struc { inner: data.unwrap() })
                }
            }
        )+
    }
}

impl_codec_for!(Message, Detail);

struct MockCrypto {
    address: Bytes,
}

impl MockCrypto {
    fn new(address: Bytes) -> Self {
        MockCrypto { address }
    }
}

impl Crypto for MockCrypto {
    fn hash(&self, msg: Bytes) -> Bytes {
        hash(&msg)
    }

    fn sign(&self, _hash: Bytes) -> Result<Bytes, Box<dyn Error + Send>> {
        // TODO: implement your signature method
        Ok(self.address.clone())
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
        signature: Bytes,
        _hash: Bytes,
    ) -> Result<Bytes, Box<dyn Error + Send>> {
        // TODO: implement your verify signature method
        Ok(signature)
    }

    fn verify_aggregated_signature(
        &self,
        _aggregated_signature: AggregatedSignature,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }
}

struct ConsensusEngine {
    authority_list: Vec<Node>,
    peers:          HashMap<Bytes, Sender<OverlordMsg<Message>>>,
    network:        Receiver<OverlordMsg<Message>>,
    commits:        Arc<Mutex<HashMap<u64, Bytes>>>,
}

impl ConsensusEngine {
    fn new(
        authority_list: Vec<Node>,
        peers: HashMap<Bytes, Sender<OverlordMsg<Message>>>,
        network: Receiver<OverlordMsg<Message>>,
        commits: Arc<Mutex<HashMap<u64, Bytes>>>,
    ) -> ConsensusEngine {
        ConsensusEngine {
            authority_list,
            peers,
            network,
            commits,
        }
    }
}

#[async_trait]
impl Consensus<Message, Detail> for ConsensusEngine {
    async fn get_epoch(
        &self,
        _ctx: Context,
        _epoch_id: u64,
    ) -> Result<(Message, Hash), Box<dyn Error + Send>> {
        let data = gen_random_bytes();
        Ok((Message::from(data.clone()), hash(&data)))
    }

    async fn check_epoch(
        &self,
        _ctx: Context,
        _epoch_id: u64,
        _hash: Hash,
        data: Message,
    ) -> Result<Detail, Box<dyn Error + Send>> {
        Ok(Detail::from(data))
    }

    async fn commit(
        &self,
        _ctx: Context,
        epoch_id: u64,
        commit: Commit<Message>,
    ) -> Result<Status, Box<dyn Error + Send>> {
        let mut commit_map = self.commits.lock().unwrap();
        if let Some(data) = commit_map.get(&commit.epoch_id) {
            assert_eq!(data, &commit.content.inner);
        } else {
            println!(
                "In epoch_id: {:?}, commit with : {:?}",
                commit.epoch_id,
                hex::encode(commit.content.inner.clone())
            );
            commit_map.insert(commit.epoch_id, commit.content.inner);
        }

        Ok(Status {
            epoch_id:       epoch_id + 1,
            interval:       Some(INTERVAL),
            authority_list: self.authority_list.clone(),
        })
    }

    async fn get_authority_list(
        &self,
        _ctx: Context,
        _epoch_id: u64,
    ) -> Result<Vec<Node>, Box<dyn Error + Send>> {
        Ok(self.authority_list.clone())
    }

    async fn broadcast_to_other(
        &self,
        _ctx: Context,
        msg: OverlordMsg<Message>,
    ) -> Result<(), Box<dyn Error + Send>> {
        self.peers.iter().for_each(|(_, sender)| {
            sender.send(msg.clone()).unwrap();
        });
        Ok(())
    }

    async fn transmit_to_relayer(
        &self,
        _ctx: Context,
        addr: Bytes,
        msg: OverlordMsg<Message>,
    ) -> Result<(), Box<dyn Error + Send>> {
        self.peers.get(&addr).unwrap().send(msg).unwrap();
        Ok(())
    }
}

struct ConsensusMachine {
    overlord: Arc<Overlord<Message, Detail, ConsensusEngine, MockCrypto>>,
    handler:  OverlordHandler<Message>,
    engine:   Arc<ConsensusEngine>,
}

impl ConsensusMachine {
    fn new(
        address: Bytes,
        authority_list: Vec<Node>,
        peers: HashMap<Bytes, Sender<OverlordMsg<Message>>>,
        network: Receiver<OverlordMsg<Message>>,
        commits: Arc<Mutex<HashMap<u64, Bytes>>>,
    ) -> Self {
        let crypto = MockCrypto::new(address.clone());
        let engine = Arc::new(ConsensusEngine::new(
            authority_list.clone(),
            peers,
            network,
            commits,
        ));
        let overlord = Overlord::new(address, Arc::clone(&engine), crypto);
        let overlord_handler = overlord.get_handler();

        overlord_handler
            .send_msg(
                Context::new(),
                OverlordMsg::RichStatus(Status {
                    epoch_id: 1,
                    interval: Some(INTERVAL),
                    authority_list,
                }),
            )
            .unwrap();

        Self {
            overlord: Arc::new(overlord),
            handler: overlord_handler,
            engine,
        }
    }

    async fn run(
        &self,
        interval: u64,
        timer_config: Option<DurationConfig>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let engine = Arc::<ConsensusEngine>::clone(&self.engine);
        let handler = self.handler.clone();

        thread::spawn(move || loop {
            if let Ok(msg) = engine.network.recv() {
                match msg {
                    OverlordMsg::SignedVote(vote) => {
                        handler
                            .send_msg(Context::new(), OverlordMsg::SignedVote(vote))
                            .unwrap();
                    }
                    OverlordMsg::SignedProposal(proposal) => {
                        handler
                            .send_msg(Context::new(), OverlordMsg::SignedProposal(proposal))
                            .unwrap();
                    }
                    OverlordMsg::AggregatedVote(agg_vote) => {
                        handler
                            .send_msg(Context::new(), OverlordMsg::AggregatedVote(agg_vote))
                            .unwrap();
                    }
                    _ => {}
                }
            }
        });

        self.overlord.run(interval, timer_config).await.unwrap();

        Ok(())
    }
}

#[runtime::main(runtime_tokio::Tokio)]
async fn main() {
    let authority_list: Vec<Node> = (0..NODE_NUM)
        .map(|_| Node::new(gen_random_bytes()))
        .collect();
    let connections: Vec<Chan> = (0..NODE_NUM).map(|_| unbounded()).collect();
    let networks: HashMap<Bytes, Receiver<OverlordMsg<Message>>> = authority_list
        .iter()
        .map(|node| node.address.clone())
        .zip(connections.iter().map(|(_, receiver)| receiver.clone()))
        .collect();
    let commits = Arc::new(Mutex::new(HashMap::new()));

    let authority_list_clone = authority_list.clone();

    for node in authority_list {
        let address = node.address;
        let mut peers: HashMap<Bytes, Sender<OverlordMsg<Message>>> = authority_list_clone
            .iter()
            .map(|node| node.address.clone())
            .zip(connections.iter().map(|(sender, _)| sender.clone()))
            .collect();
        peers.remove(&address);

        let node = Arc::new(ConsensusMachine::new(
            address.clone(),
            authority_list_clone.clone(),
            peers,
            networks.get(&address).unwrap().clone(),
            Arc::<Mutex<HashMap<u64, Bytes>>>::clone(&commits),
        ));
        runtime::spawn(async move {
            node.run(INTERVAL, timer_config()).await.unwrap();
        });
    }

    thread::sleep(Duration::from_secs(100));
}

fn gen_random_bytes() -> Bytes {
    let vec: Vec<u8> = (0..10).map(|_| random::<u8>()).collect();
    Bytes::from(vec)
}

fn hash(bytes: &Bytes) -> Bytes {
    let mut out = [0u8; 32];
    out.copy_from_slice(&HASHER_INST.digest(bytes));
    Bytes::from(&out[..])
}

fn timer_config() -> Option<DurationConfig> {
    Some(DurationConfig::new(10, 10, 10, 10, 10, 10))
}
