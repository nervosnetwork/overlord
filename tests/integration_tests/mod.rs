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
use rand::{random, seq::SliceRandom, thread_rng};
use serde::{Deserialize, Serialize};

use overlord::types::{Commit, Hash, Node, OverlordMsg, Status};
use overlord::{Codec, Consensus, Crypto, DurationConfig, Overlord, OverlordHandler, Wal};
use std::sync::atomic::{AtomicBool, Ordering};

lazy_static! {
    static ref HASHER_INST: HasherKeccak = HasherKeccak::new();
}

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
                    Ok(self.inner.clone())
                }

                fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>> {
                    Ok($struc { inner: data })
                }
            }
        )+
    }
}

impl_codec_for!(Speech, Detail);

#[test]
fn test_speech_codec() {
    for _ in 0..100 {
        let thought = gen_random_bytes();
        let speech = Speech::from(thought);

        let decode: Speech = Codec::decode(Codec::encode(&speech).unwrap()).unwrap();
        assert_eq!(decode, speech);
    }
}

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
    name:          Bytes, // address
    speaker_list:  Vec<Node>,
    talk_to:       HashMap<Bytes, Sender<OverlordMsg<Speech>>>,
    hearing:       Receiver<OverlordMsg<Speech>>,
    commit_record: Arc<Mutex<HashMap<u64, Bytes>>>, // height => speech
    height_record: Arc<Mutex<HashMap<Bytes, u64>>>, // address => height
    interval:      u64,
}

impl Brain {
    fn new(
        name: Bytes,
        speaker_list: Vec<Node>,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Speech>>>,
        hearing: Receiver<OverlordMsg<Speech>>,
        commit_record: Arc<Mutex<HashMap<u64, Bytes>>>,
        height_record: Arc<Mutex<HashMap<Bytes, u64>>>,
        interval: u64,
    ) -> Brain {
        Brain {
            name,
            speaker_list,
            talk_to,
            hearing,
            commit_record,
            height_record,
            interval,
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
        let mut commit_record = self.commit_record.lock().unwrap();
        if let Some(speech) = commit_record.get(&commit.height) {
            // Consistency check
            assert_eq!(speech, &commit.content.inner);
        } else {
            println!(
                "node {:?} first commit in height: {:?}",
                get_index(&self.speaker_list, &self.name),
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
            let _ = mouth.send(words.clone());
        });
        Ok(())
    }

    async fn transmit_to_relayer(
        &self,
        _ctx: Context,
        name: Bytes,
        words: OverlordMsg<Speech>,
    ) -> Result<(), Box<dyn Error + Send>> {
        if let Some(sender) = self.talk_to.get(&name) {
            let _ = sender.send(words);
        }
        Ok(())
    }
}

struct Speaker {
    overlord: Arc<Overlord<Speech, Brain, MockCrypto, MockWal>>,
    handler:  OverlordHandler<Speech>,
    brain:    Arc<Brain>,
    stopped:  Arc<AtomicBool>,
}

impl Speaker {
    fn new(
        info: (Bytes, u64),
        speaker_list: Vec<Node>,
        talk_to: HashMap<Bytes, Sender<OverlordMsg<Speech>>>,
        hearing: Receiver<OverlordMsg<Speech>>,
        commit_record: Arc<Mutex<HashMap<u64, Bytes>>>,
        height_record: Arc<Mutex<HashMap<Bytes, u64>>>,
        wal: Arc<MockWal>,
    ) -> Self {
        let (name, interval) = info;
        let crypto = MockCrypto::new(name.clone());
        let brain = Arc::new(Brain::new(
            name.clone(),
            speaker_list.clone(),
            talk_to,
            hearing,
            commit_record,
            height_record,
            interval,
        ));
        let overlord = Overlord::new(name, Arc::clone(&brain), Arc::new(crypto), wal);
        let overlord_handler = overlord.get_handler();

        overlord_handler
            .send_msg(
                Context::new(),
                OverlordMsg::RichStatus(Status {
                    height:         1,
                    interval:       Some(interval),
                    timer_config:   timer_config(),
                    authority_list: speaker_list,
                }),
            )
            .unwrap();

        Self {
            overlord: Arc::new(overlord),
            handler: overlord_handler,
            brain,
            stopped: Arc::new(AtomicBool::new(false)),
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
        let stopped = Arc::<AtomicBool>::clone(&self.stopped);

        thread::spawn(move || loop {
            if let Ok(msg) = brain.hearing.recv() {
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
                        // println!("node {:?}, {:?}, receive {:?}", get_index(&brain.speaker_list,
                        // &brain.name), brain.name,choke);
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
            .run(interval, speaker_list, timer_config)
            .await
            .unwrap();

        Ok(())
    }
}

async fn run_wal_test(num: usize, interval: u64, change_nodes_cycles: u64, test_height: u64) {
    let speakers: Vec<Node> = (0..num).map(|_| Node::new(gen_random_bytes())).collect();
    println!("Wal test start, generate {:?} speakers", num);
    let wal_record: HashMap<Bytes, Arc<MockWal>> = (0..num)
        .map(|i| {
            (
                speakers.get(i).unwrap().address.clone(),
                Arc::new(MockWal::new()),
            )
        })
        .collect();
    let commit_record: Arc<Mutex<HashMap<u64, Bytes>>> = Arc::new(Mutex::new(HashMap::new()));
    let height_record: Arc<Mutex<HashMap<Bytes, u64>>> = Arc::new(Mutex::new(HashMap::new()));

    let mut test_count = 0;
    loop {
        let height_start = get_max_height(Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record));

        let alive_speakers = if test_count == 0 {
            speakers.clone()
        } else {
            gen_alive_nodes(speakers.clone())
        };
        let alive_num = alive_speakers.len();
        println!(
            "Cycle {:?} start, generate {:?} alive_speakers of {:?}",
            test_count,
            alive_num,
            get_index_array(&speakers, &alive_speakers)
        );

        let channels: Vec<Channel> = (0..alive_num).map(|_| unbounded()).collect();
        let hearings: HashMap<Bytes, Receiver<OverlordMsg<Speech>>> = alive_speakers
            .iter()
            .map(|node| node.address.clone())
            .zip(channels.iter().map(|(_, receiver)| receiver.clone()))
            .collect();

        let alive_speakers_clone = alive_speakers.clone();
        let auth_list = speakers.clone();
        let mut handlers = Vec::new();
        for speaker in alive_speakers.iter() {
            let name = speaker.address.clone();
            let mut talk_to: HashMap<Bytes, Sender<OverlordMsg<Speech>>> = alive_speakers_clone
                .iter()
                .map(|speaker| speaker.address.clone())
                .zip(channels.iter().map(|(sender, _)| sender.clone()))
                .collect();
            talk_to.remove(&name);

            let speaker = Arc::new(Speaker::new(
                (name.clone(), interval),
                auth_list.clone(),
                talk_to,
                hearings.get(&name).unwrap().clone(),
                Arc::<Mutex<HashMap<u64, Bytes>>>::clone(&commit_record),
                Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record),
                Arc::<MockWal>::clone(wal_record.get(&name).unwrap()),
            ));

            handlers.push(Arc::<Speaker>::clone(&speaker));

            let list = auth_list.clone();
            tokio::spawn(async move {
                speaker.run(interval, timer_config(), list).await.unwrap();
            });
        }

        // synchronization height
        let height_record_clone = Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record);
        let handles_clone = handlers.clone();
        let speakers_clone = speakers.clone();
        tokio::spawn(async move {
            thread::sleep(Duration::from_millis(interval));
            let max_height = get_max_height(Arc::<Mutex<HashMap<Bytes, u64>>>::clone(
                &height_record_clone,
            ));
            {
                let height_record = height_record_clone.lock().unwrap();
                height_record.iter().for_each(|(name, height)| {
                    // println!("node {:?} in height {:?}", get_index(&speakers_clone, name),
                    // height);
                    if *height < max_height - 1 {
                        handles_clone
                            .iter()
                            .filter(|speaker| speaker.brain.name == name)
                            .for_each(|speaker| {
                                println!(
                                    "synchronize {:?} to node {:?} of height {:?}",
                                    max_height + 1,
                                    get_index(&speakers_clone, name),
                                    height
                                );
                                let _ = speaker.handler.send_msg(
                                    Context::new(),
                                    OverlordMsg::RichStatus(Status {
                                        height:         max_height + 1,
                                        interval:       Some(interval),
                                        timer_config:   timer_config(),
                                        authority_list: speakers_clone.clone(),
                                    }),
                                );
                            });
                    }
                });
            }
        });

        let mut height_end =
            get_max_height(Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record));
        while height_end - height_start < change_nodes_cycles {
            thread::sleep(Duration::from_millis(interval));
            height_end = get_max_height(Arc::<Mutex<HashMap<Bytes, u64>>>::clone(&height_record));
        }
        println!(
            "Cycle {:?} start from {:?}, end with {:?}",
            test_count, height_start, height_end
        );

        // close consensus process
        println!("Cycle {:?} end, kill alive-speakers", test_count);
        handlers.iter().for_each(|speaker| {
            speaker.stopped.store(true, Ordering::Relaxed);
            speaker
                .handler
                .send_msg(Context::new(), OverlordMsg::Stop)
                .unwrap()
        });

        test_count += 1;

        if height_end > test_height {
            break;
        }
    }
}

#[tokio::test(threaded_scheduler)]
async fn test_1_wal() {
    run_wal_test(1, 100, 1, 1).await
}

#[tokio::test(threaded_scheduler)]
async fn test_3_wal() {
    run_wal_test(3, 100, 1, 1).await
}

#[tokio::test(threaded_scheduler)]
async fn test_4_wal() {
    // let _ = env_logger::builder().is_test(true).try_init();
    run_wal_test(4, 100, 1, 1).await
}

#[tokio::test(threaded_scheduler)]
async fn test_21_wal() {
    run_wal_test(21, 100, 1, 1).await
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
    Some(DurationConfig::new(20, 10, 5, 10))
}

fn gen_alive_nodes(nodes: Vec<Node>) -> Vec<Node> {
    let node_num = nodes.len();
    let thresh_num = node_num * 2 / 3 + 1;
    let rand_num = 0;
    //    let rand_num = random::<usize>() % (node_num - thresh_num + 1);
    let mut alive_nodes = nodes;
    alive_nodes.shuffle(&mut thread_rng());
    while alive_nodes.len() > thresh_num + rand_num {
        alive_nodes.pop();
    }
    alive_nodes
}

fn get_max_height(height_record: Arc<Mutex<HashMap<Bytes, u64>>>) -> u64 {
    let height_record = height_record.lock().unwrap();
    if let Some(max_height) = height_record.values().max() {
        *max_height
    } else {
        0
    }
}

fn get_index_array(nodes: &[Node], alives: &[Node]) -> Vec<usize> {
    nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| alives.contains(node))
        .map(|(i, _)| i)
        .collect()
}

fn get_index(nodes: &[Node], name: &Bytes) -> usize {
    let mut index = std::usize::MAX;
    nodes.iter().enumerate().for_each(|(i, node)| {
        if node.address == name {
            index = i;
        }
    });
    index
}
