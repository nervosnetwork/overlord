use std::collections::HashMap;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use async_trait::async_trait;
use bytes::Bytes;
use creep::Context;
use crossbeam_channel::{Receiver, Sender};
use lru_cache::LruCache;
use serde::{Deserialize, Serialize};

use overlord::types::{Commit, Hash, Node, OverlordMsg, Status};
use overlord::{Codec, Consensus, DurationConfig, Overlord, OverlordHandler};

use super::crypto::MockCrypto;
use super::utils::{gen_random_bytes, get_index, hash, timer_config};
use super::wal::MockWal;

pub type Channel = (Sender<OverlordMsg<Block>>, Receiver<OverlordMsg<Block>>);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Block {
    inner: Bytes,
}

impl Block {
    fn from(content: Bytes) -> Self {
        Block { inner: content }
    }
}

impl Codec for Block {
    fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
        Ok(self.inner.clone())
    }

    fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>> {
        Ok(Block { inner: data })
    }
}

pub struct Adapter {
    pub name:          Bytes, // address
    pub node_list:     Vec<Node>,
    pub talk_to:       HashMap<Bytes, Sender<OverlordMsg<Block>>>,
    pub hearing:       Receiver<OverlordMsg<Block>>,
    pub commit_record: Arc<Mutex<LruCache<u64, Bytes>>>, // height => Block
    pub height_record: Arc<Mutex<HashMap<Bytes, u64>>>,  // address => height
    pub interval:      u64,
}

impl Adapter {
    fn new(
        name: Bytes,
        node_list: Vec<Node>,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>>,
        hearing: Receiver<OverlordMsg<Block>>,
        commit_record: Arc<Mutex<LruCache<u64, Bytes>>>,
        height_record: Arc<Mutex<HashMap<Bytes, u64>>>,
        interval: u64,
    ) -> Adapter {
        Adapter {
            name,
            node_list,
            talk_to,
            hearing,
            commit_record,
            height_record,
            interval,
        }
    }
}

#[async_trait]
impl Consensus<Block> for Adapter {
    async fn get_block(
        &self,
        _ctx: Context,
        _height: u64,
    ) -> Result<(Block, Hash), Box<dyn Error + Send>> {
        let content = gen_random_bytes();
        Ok((Block::from(content.clone()), hash(&content)))
    }

    async fn check_block(
        &self,
        _ctx: Context,
        _height: u64,
        _hash: Hash,
        _block: Block,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }

    async fn commit(
        &self,
        _ctx: Context,
        height: u64,
        commit: Commit<Block>,
    ) -> Result<Status, Box<dyn Error + Send>> {
        let mut commit_record = self.commit_record.lock().unwrap();
        if let Some(block) = commit_record.get_mut(&commit.height) {
            // Consistency check
            assert_eq!(block, &commit.content.inner);
        } else {
            println!(
                "node {:?} first commit in height: {:?}",
                get_index(&self.node_list, &self.name),
                commit.height,
            );
            commit_record.insert(commit.height, commit.content.inner);
        }

        let mut height_record = self.height_record.lock().unwrap();
        height_record.insert(self.name.clone(), commit.height);

        Ok(Status {
            height:         height + 1,
            interval:       Some(self.interval),
            timer_config:   None,
            authority_list: self.node_list.clone(),
        })
    }

    async fn get_authority_list(
        &self,
        _ctx: Context,
        _height: u64,
    ) -> Result<Vec<Node>, Box<dyn Error + Send>> {
        Ok(self.node_list.clone())
    }

    async fn broadcast_to_other(
        &self,
        _ctx: Context,
        words: OverlordMsg<Block>,
    ) -> Result<(), Box<dyn Error + Send>> {
        self.talk_to.iter().for_each(|(_, mouth)| {
            let _ = mouth.send(words.clone());
        });
        Ok(())
    }

    async fn transmit_to_relayer(
        &self,
        _ctx: Context,
        name: Bytes,
        words: OverlordMsg<Block>,
    ) -> Result<(), Box<dyn Error + Send>> {
        if let Some(sender) = self.talk_to.get(&name) {
            let _ = sender.send(words);
        }
        Ok(())
    }
}

pub struct Participant {
    pub overlord: Arc<Overlord<Block, Adapter, MockCrypto, MockWal>>,
    pub handler:  OverlordHandler<Block>,
    pub adapter:  Arc<Adapter>,
    pub stopped:  Arc<AtomicBool>,
}

impl Participant {
    pub fn new(
        info: (Bytes, u64),
        node_list: Vec<Node>,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>>,
        hearing: Receiver<OverlordMsg<Block>>,
        commit_record: Arc<Mutex<LruCache<u64, Bytes>>>,
        height_record: Arc<Mutex<HashMap<Bytes, u64>>>,
        wal: Arc<MockWal>,
    ) -> Self {
        let (name, interval) = info;
        let crypto = MockCrypto::new(name.clone());
        let adapter = Arc::new(Adapter::new(
            name.clone(),
            node_list.clone(),
            talk_to,
            hearing,
            commit_record,
            height_record,
            interval,
        ));
        let overlord = Overlord::new(name, Arc::clone(&adapter), Arc::new(crypto), wal);
        let overlord_handler = overlord.get_handler();

        overlord_handler
            .send_msg(
                Context::new(),
                OverlordMsg::RichStatus(Status {
                    height:         1,
                    interval:       Some(interval),
                    timer_config:   timer_config(),
                    authority_list: node_list,
                }),
            )
            .unwrap();

        Self {
            overlord: Arc::new(overlord),
            handler: overlord_handler,
            adapter,
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn run(
        &self,
        interval: u64,
        timer_config: Option<DurationConfig>,
        node_list: Vec<Node>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let adapter = Arc::<Adapter>::clone(&self.adapter);
        let handler = self.handler.clone();
        let stopped = Arc::<AtomicBool>::clone(&self.stopped);

        thread::spawn(move || loop {
            if let Ok(msg) = adapter.hearing.recv() {
                match msg {
                    OverlordMsg::SignedVote(vote) => {
                        let _ = handler.send_msg(Context::new(), OverlordMsg::SignedVote(vote));
                    }
                    OverlordMsg::SignedProposal(proposal) => {
                        let _ =
                            handler.send_msg(Context::new(), OverlordMsg::SignedProposal(proposal));
                    }
                    OverlordMsg::AggregatedVote(agg_vote) => {
                        let _ =
                            handler.send_msg(Context::new(), OverlordMsg::AggregatedVote(agg_vote));
                    }
                    OverlordMsg::SignedChoke(choke) => {
                        let _ = handler.send_msg(Context::new(), OverlordMsg::SignedChoke(choke));
                    }
                    _ => {}
                }
            }
            if stopped.load(Ordering::Relaxed) {
                break;
            }
        });

        self.overlord
            .run(interval, node_list, timer_config)
            .await
            .unwrap();

        Ok(())
    }
}

#[test]
fn test_block_codec() {
    for _ in 0..100 {
        let content = gen_random_bytes();
        let block = Block::from(content);

        let decode: Block = Codec::decode(Codec::encode(&block).unwrap()).unwrap();
        assert_eq!(decode, block);
    }
}
