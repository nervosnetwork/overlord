use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use creep::Context;
use crossbeam_channel::{unbounded, Receiver, Sender};
use hasher::{Hasher, HasherKeccak};
use lazy_static::lazy_static;
use rand::random;
use serde::{Deserialize, Serialize};

use overlord::error::ConsensusError;
use overlord::types::{Commit, Hash, Node, OverlordMsg, Status};
use overlord::{Codec, Consensus, Crypto, DurationConfig, Overlord, OverlordHandler, Wal};

lazy_static! {
    static ref HASHER_INST: HasherKeccak = HasherKeccak::new();
}

const SPEAKER_NUM: u8 = 10;

const SPEECH_INTERVAL: u64 = 1000; // ms

type Channel = (Sender<OverlordMsg<Speech>>, Receiver<OverlordMsg<Speech>>);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Speech {
    inner: Bytes,
}

impl Speech {
    fn from(thought: Bytes) -> Self {
        Speech { inner: thought }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Detail {
    inner: Bytes,
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

impl_codec_for!(Speech, Detail);

struct MockWal {
    inner: Mutex<Option<Bytes>>,
}

impl MockWal {
    fn new() -> MockWal {
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

struct MockCrypto {
    name: Bytes,
}

impl MockCrypto {
    fn new(name: Bytes) -> Self {
        MockCrypto { name }
    }
}

impl Crypto for MockCrypto {
    fn hash(&self, speech: Bytes) -> Bytes {
        hash(&speech)
    }

    fn sign(&self, _hash: Bytes) -> Result<Bytes, Box<dyn Error + Send>> {
        Ok(self.name.clone())
    }

    fn aggregate_signatures(
        &self,
        _signatures: Vec<Bytes>,
        _speaker: Vec<Bytes>,
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

struct Brain {
    speaker_list:     Vec<Node>,
    talk_to:          HashMap<Bytes, Sender<OverlordMsg<Speech>>>,
    hearing:          Receiver<OverlordMsg<Speech>>,
    consensus_speech: Arc<Mutex<HashMap<u64, Bytes>>>,
}

impl Brain {
    fn new(
        speaker_list: Vec<Node>,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Speech>>>,
        hearing: Receiver<OverlordMsg<Speech>>,
        consensus_speech: Arc<Mutex<HashMap<u64, Bytes>>>,
    ) -> Brain {
        Brain {
            speaker_list,
            talk_to,
            hearing,
            consensus_speech,
        }
    }
}

#[async_trait]
impl Consensus<Speech> for Brain {
    async fn get_block(
        &self,
        _ctx: Context,
        _height: u64,
    ) -> Result<(Speech, Hash), Box<dyn Error + Send>> {
        let thought = gen_random_bytes();
        Ok((Speech::from(thought.clone()), hash(&thought)))
    }

    async fn check_block(
        &self,
        _ctx: Context,
        _height: u64,
        _hash: Hash,
        _speech: Speech,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }

    async fn commit(
        &self,
        _ctx: Context,
        height: u64,
        commit: Commit<Speech>,
    ) -> Result<Status, Box<dyn Error + Send>> {
        let mut speeches = self.consensus_speech.lock().unwrap();
        if let Some(speech) = speeches.get(&commit.height) {
            assert_eq!(speech, &commit.content.inner);
        } else {
            println!(
                "In height: {:?}, commit with : {:?}",
                commit.height,
                hex::encode(commit.content.inner.clone())
            );
            speeches.insert(commit.height, commit.content.inner);
        }

        Ok(Status {
            height:         height + 1,
            interval:       Some(SPEECH_INTERVAL),
            timer_config:   None,
            authority_list: self.speaker_list.clone(),
        })
    }

    async fn get_authority_list(
        &self,
        _ctx: Context,
        _height: u64,
    ) -> Result<Vec<Node>, Box<dyn Error + Send>> {
        Ok(self.speaker_list.clone())
    }

    async fn broadcast_to_other(
        &self,
        _ctx: Context,
        words: OverlordMsg<Speech>,
    ) -> Result<(), Box<dyn Error + Send>> {
        self.talk_to.iter().for_each(|(_, mouth)| {
            mouth.send(words.clone()).unwrap();
        });
        Ok(())
    }

    async fn transmit_to_relayer(
        &self,
        _ctx: Context,
        name: Bytes,
        words: OverlordMsg<Speech>,
    ) -> Result<(), Box<dyn Error + Send>> {
        self.talk_to.get(&name).unwrap().send(words).unwrap();
        Ok(())
    }

    fn report_error(&self, _ctx: Context, _err: ConsensusError) {}
}

struct Speaker {
    overlord: Arc<Overlord<Speech, Brain, MockCrypto, MockWal>>,
    handler:  OverlordHandler<Speech>,
    brain:    Arc<Brain>,
}

impl Speaker {
    fn new(
        name: Bytes,
        speaker_list: Vec<Node>,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Speech>>>,
        hearing: Receiver<OverlordMsg<Speech>>,
        consensus_speech: Arc<Mutex<HashMap<u64, Bytes>>>,
    ) -> Self {
        let crypto = MockCrypto::new(name.clone());
        let brain = Arc::new(Brain::new(
            speaker_list.clone(),
            talk_to,
            hearing,
            consensus_speech,
        ));
        let overlord = Overlord::new(
            name,
            Arc::clone(&brain),
            Arc::new(crypto),
            Arc::new(MockWal::new()),
        );
        let overlord_handler = overlord.get_handler();

        overlord_handler
            .send_msg(
                Context::new(),
                OverlordMsg::RichStatus(Status {
                    height:         1,
                    interval:       Some(SPEECH_INTERVAL),
                    timer_config:   None,
                    authority_list: speaker_list,
                }),
            )
            .unwrap();

        Self {
            overlord: Arc::new(overlord),
            handler: overlord_handler,
            brain,
        }
    }

    async fn run(
        &self,
        interval: u64,
        timer_config: Option<DurationConfig>,
        speaker_list: Vec<Node>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let brain = Arc::<Brain>::clone(&self.brain);
        let handler = self.handler.clone();

        thread::spawn(move || loop {
            if let Ok(msg) = brain.hearing.recv() {
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
                    OverlordMsg::SignedChoke(choke) => {
                        handler
                            .send_msg(Context::new(), OverlordMsg::SignedChoke(choke))
                            .unwrap();
                    }
                    _ => {}
                }
            }
        });

        self.overlord
            .run(interval, speaker_list, timer_config)
            .await
            .unwrap();

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let speaker_list: Vec<Node> = (0..SPEAKER_NUM)
        .map(|_| Node::new(gen_random_bytes()))
        .collect();
    let channels: Vec<Channel> = (0..SPEAKER_NUM).map(|_| unbounded()).collect();
    let hearings: HashMap<Bytes, Receiver<OverlordMsg<Speech>>> = speaker_list
        .iter()
        .map(|node| node.address.clone())
        .zip(channels.iter().map(|(_, receiver)| receiver.clone()))
        .collect();
    let consensus_speech = Arc::new(Mutex::new(HashMap::new()));

    let speaker_list_clone = speaker_list.clone();
    let auth_list = speaker_list.clone();

    for speaker in speaker_list.iter() {
        let name = speaker.address.clone();
        let mut talk_to: HashMap<Bytes, Sender<OverlordMsg<Speech>>> = speaker_list_clone
            .iter()
            .map(|speaker| speaker.address.clone())
            .zip(channels.iter().map(|(sender, _)| sender.clone()))
            .collect();
        talk_to.remove(&name);

        let speaker = Arc::new(Speaker::new(
            name.clone(),
            speaker_list_clone.clone(),
            talk_to,
            hearings.get(&name).unwrap().clone(),
            Arc::<Mutex<HashMap<u64, Bytes>>>::clone(&consensus_speech),
        ));

        let list = auth_list.clone();
        tokio::spawn(async move {
            speaker
                .run(SPEECH_INTERVAL, timer_config(), list)
                .await
                .unwrap();
        });
    }

    thread::sleep(Duration::from_secs(10));
}

fn gen_random_bytes() -> Bytes {
    let vec: Vec<u8> = (0..10).map(|_| random::<u8>()).collect();
    Bytes::from(vec)
}

fn hash(bytes: &Bytes) -> Bytes {
    let mut out = [0u8; 32];
    out.copy_from_slice(&HASHER_INST.digest(bytes));
    BytesMut::from(&out[..]).freeze()
}

fn timer_config() -> Option<DurationConfig> {
    Some(DurationConfig::new(10, 10, 10, 3))
}
