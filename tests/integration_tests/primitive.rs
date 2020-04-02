use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::thread;

use async_trait::async_trait;
use bytes::Bytes;
use creep::Context;
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

use overlord::error::ConsensusError;
use overlord::types::{Commit, Hash, Node, OverlordMsg, Status};
use overlord::{Codec, Consensus, DurationConfig, Overlord, OverlordHandler};

use super::crypto::MockCrypto;
use super::utils::{gen_random_bytes, hash, timer_config, to_hex};
use super::wal::{MockWal, RECORD_TMP_FILE};
use crate::integration_tests::wal::RecordInternal;

pub type Channel = (Sender<OverlordMsg<Block>>, Receiver<OverlordMsg<Block>>);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Block {
    #[serde(with = "overlord::serde_hex")]
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
    pub address: Bytes, // address
    pub talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>>,
    pub hearing: Receiver<OverlordMsg<Block>>,
    pub records: RecordInternal,
}

impl Adapter {
    fn new(
        address: Bytes,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>>,
        hearing: Receiver<OverlordMsg<Block>>,
        records: RecordInternal,
    ) -> Adapter {
        Adapter {
            address,
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
        let status = Status {
            height:         height + 1,
            interval:       Some(self.records.interval),
            timer_config:   None,
            authority_list: self.records.node_record.clone(),
        };

        let commit_block_hash = hash(&commit.content.inner);

        {
            let test_id_updated = *self.records.test_id_updated.lock().unwrap();
            // avoid previous test overwrite commit_records and height_records of the latest test
            if test_id_updated != self.records.test_id {
                println!("Previous test try to overwrite records");
                return Ok(status);
            }
        }

        let mut consistency_break = false;
        {
            let mut commit_record = self.records.commit_record.lock().unwrap();
            if let Some(block_hash) = commit_record.get_mut(&commit.height) {
                // Consistency check
                if block_hash != &commit_block_hash {
                    consistency_break = true;
                }
            } else {
                println!(
                    "node {:?} first commit in height: {:?}",
                    to_hex(&self.address),
                    commit.height,
                );
            }
        }

        if consistency_break {
            self.records.save(RECORD_TMP_FILE);
            panic!("Consistency break!!");
        } else {
            println!(
                "node {:?} commit in height: {:?}",
                to_hex(&self.address),
                commit.height,
            );
            let mut commit_record = self.records.commit_record.lock().unwrap();
            commit_record.insert(commit.height, commit_block_hash);
            let mut height_record = self.records.height_record.lock().unwrap();
            height_record.insert(self.address.clone(), commit.height);
        }

        Ok(status)
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
        address: Bytes,
        words: OverlordMsg<Block>,
    ) -> Result<(), Box<dyn Error + Send>> {
        if let Some(sender) = self.talk_to.get(&address) {
            let _ = sender.send(words);
        }
        Ok(())
    }

    fn report_error(&self, _ctx: Context, _err: ConsensusError) {}
}

pub struct Participant {
    pub overlord: Arc<Overlord<Block, Adapter, MockCrypto, MockWal>>,
    pub handler:  OverlordHandler<Block>,
    pub adapter:  Arc<Adapter>,
}

impl Participant {
    pub fn new(
        address: &Bytes,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Block>>>,
        hearing: Receiver<OverlordMsg<Block>>,
        records: RecordInternal,
    ) -> Self {
        let crypto = MockCrypto::new(address.clone());
        let adapter = Arc::new(Adapter::new(
            address.clone(),
            talk_to,
            hearing,
            records.clone(),
        ));
        let overlord = Overlord::new(
            address.clone(),
            Arc::clone(&adapter),
            Arc::new(crypto),
            Arc::new(records.wal_record.get(address).unwrap().clone()),
        );
        let overlord_handler = overlord.get_handler();

        overlord_handler
            .send_msg(
                Context::new(),
                OverlordMsg::RichStatus(Status {
                    height:         1,
                    interval:       Some(records.interval),
                    timer_config:   timer_config(),
                    authority_list: records.node_record,
                }),
            )
            .unwrap();

        Self {
            overlord: Arc::new(overlord),
            handler: overlord_handler,
            adapter,
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
                    OverlordMsg::Stop => {
                        break;
                    }
                    _ => {}
                }
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
