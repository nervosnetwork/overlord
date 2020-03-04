use std::collections::HashMap;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use async_trait::async_trait;
use bytes::Bytes;
use creep::Context;
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

use overlord::types::{Commit, Hash, Node, OverlordMsg, Status};
use overlord::{Codec, Consensus, DurationConfig, Overlord, OverlordHandler};

use super::crypto::MockCrypto;
use super::utils::{gen_random_bytes, get_index, hash, timer_config};
use super::wal::{MockWal, Record, RECORD_TMP_FILE};

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
    pub name:    Bytes, // address
    pub talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>>,
    pub hearing: Receiver<OverlordMsg<Block>>,
    pub records: Arc<Record>,
}

impl Adapter {
    fn new(
        name: Bytes,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>>,
        hearing: Receiver<OverlordMsg<Block>>,
        records: Arc<Record>,
    ) -> Adapter {
        Adapter {
            name,
            talk_to,
            hearing,
            records,
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
        let mut commit_record = self.records.commit_record.lock().unwrap();
        if let Some(block) = commit_record.get_mut(&commit.height) {
            // Consistency check
            if block != &commit.content.inner {
                println!("Consistency break!!");
                self.records.save(RECORD_TMP_FILE);
            }
            assert_eq!(block, &commit.content.inner);
        } else {
            println!(
                "node {:?} first commit in height: {:?}",
                get_index(&self.records.node_record, &self.name),
                commit.height,
            );
            commit_record.insert(commit.height, commit.content.inner);
        }

        let mut height_record = self.records.height_record.lock().unwrap();
        height_record.insert(self.name.clone(), commit.height);

        Ok(Status {
            height:         height + 1,
            interval:       Some(self.records.interval),
            timer_config:   None,
            authority_list: self.records.node_record.clone(),
        })
    }

    async fn get_authority_list(
        &self,
        _ctx: Context,
        _height: u64,
    ) -> Result<Vec<Node>, Box<dyn Error + Send>> {
        Ok(self.records.node_record.clone())
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
        name: &Bytes,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>>,
        hearing: Receiver<OverlordMsg<Block>>,
        records: &Arc<Record>,
    ) -> Self {
        let crypto = MockCrypto::new(name.clone());
        let adapter = Arc::new(Adapter::new(
            name.clone(),
            talk_to,
            hearing,
            Arc::<Record>::clone(records),
        ));
        let overlord = Overlord::new(
            name.clone(),
            Arc::clone(&adapter),
            Arc::new(crypto),
            Arc::clone(records.wal_record.get(name).unwrap()),
        );
        let overlord_handler = overlord.get_handler();

        overlord_handler
            .send_msg(
                Context::new(),
                OverlordMsg::RichStatus(Status {
                    height:         1,
                    interval:       Some(records.interval),
                    timer_config:   timer_config(),
                    authority_list: records.node_record.clone(),
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
