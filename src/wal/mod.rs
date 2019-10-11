#![allow(dead_code)]

pub mod wal_types;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use lazy_static::lazy_static;
use rlp::{decode, Encodable};
use rocksdb::{ColumnFamily, Options, DB};

use crate::smr::smr_types::Step;
use crate::types::{AggregatedSignature, Proposal};
use crate::wal::wal_types::{ValidatorList, WalEpochId, WalRound, WalStep};
use crate::{Codec, ConsensusError, ConsensusResult, INIT_EPOCH_ID, INIT_ROUND};

pub use wal_types::{WalMsg, WalStatus};

const W_VALIDATORS: &str = "w1";
const W_EPOCH_ID: &str = "w2";
const W_ROUND: &str = "w3";
const W_STEP: &str = "w4";
const W_PROPOSAL: &str = "w5";
const W_FULLTXS: &str = "w6";
const W_QC: &str = "w7";

#[rustfmt::skip]
lazy_static! {
    static ref WAL_MAP: HashMap<&'static str, Vec<u8>> = {
        let mut map = HashMap::new();
        map.insert(W_VALIDATORS, Bytes::from_static(b"validators").as_ref().to_vec());
        map.insert(W_EPOCH_ID, Bytes::from_static(b"epoch_id").as_ref().to_vec());
        map.insert(W_ROUND, Bytes::from_static(b"round").as_ref().to_vec());
        map.insert(W_STEP, Bytes::from_static(b"step").as_ref().to_vec());
        map.insert(W_PROPOSAL, Bytes::from_static(b"proposal").as_ref().to_vec());
        map.insert(W_FULLTXS, Bytes::from_static(b"full_transcations").as_ref().to_vec());
        map.insert(W_QC, Bytes::from_static(b"qc").as_ref().to_vec());
        map
    };
}

#[derive(Debug)]
pub struct Wal {
    epoch_id: u64,
    wal:      Arc<DB>,
}

impl Wal {
    pub fn new<P: AsRef<Path>>(path: P) -> ConsensusResult<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let categories = [
            W_VALIDATORS,
            W_EPOCH_ID,
            W_ROUND,
            W_STEP,
            W_PROPOSAL,
            W_FULLTXS,
            W_QC,
        ];

        let wal = DB::open_cf(&opts, path, categories.iter())
            .map_err(|e| ConsensusError::WalErr(e.to_string()))?;

        let epoch_id_cf = get_column(&wal, W_EPOCH_ID)?;
        let key = WAL_MAP
            .get(W_EPOCH_ID)
            .ok_or_else(|| ConsensusError::WalErr("Invalid key".to_string()))?;
        let epoch_id = if let Some(tmp) = wal
            .get_cf(epoch_id_cf, key)
            .map_err(|e| ConsensusError::WalErr(e.to_string()))?
        {
            let id: WalEpochId = decode(&tmp).map_err(|e| ConsensusError::WalErr(e.to_string()))?;
            id.0
        } else {
            INIT_EPOCH_ID
        };

        Ok(Wal {
            epoch_id,
            wal: Arc::new(wal),
        })
    }

    pub fn save<P: Codec, C: Codec>(&mut self, msg: WalMsg<P, C>) -> ConsensusResult<()> {
        let (column, key, value) = match msg {
            WalMsg::Authority(nodes) => (
                get_column(&self.wal, W_VALIDATORS)?,
                WAL_MAP
                    .get(W_VALIDATORS)
                    .ok_or_else(|| ConsensusError::WalErr("Invalid key".to_string()))?,
                ValidatorList::new(nodes).rlp_bytes(),
            ),
            WalMsg::EpochId(id) => {
                if id > self.epoch_id {
                    self.epoch_id = id;
                    self.clear_all()?;
                }

                (
                    get_column(&self.wal, W_EPOCH_ID)?,
                    WAL_MAP
                        .get(W_EPOCH_ID)
                        .ok_or_else(|| ConsensusError::WalErr("Invalid key".to_string()))?,
                    WalEpochId::new(id).rlp_bytes(),
                )
            }
            WalMsg::Round(r) => (
                get_column(&self.wal, W_ROUND)?,
                WAL_MAP
                    .get(W_ROUND)
                    .ok_or_else(|| ConsensusError::WalErr("Invalid key".to_string()))?,
                WalRound::new(r).rlp_bytes(),
            ),
            WalMsg::Step(s) => (
                get_column(&self.wal, W_STEP)?,
                WAL_MAP
                    .get(W_STEP)
                    .ok_or_else(|| ConsensusError::WalErr("Invalid key".to_string()))?,
                WalStep::from(s).rlp_bytes(),
            ),
            WalMsg::Proposal(p) => (
                get_column(&self.wal, W_PROPOSAL)?,
                WAL_MAP
                    .get(W_PROPOSAL)
                    .ok_or_else(|| ConsensusError::WalErr("Invalid key".to_string()))?,
                p.rlp_bytes(),
            ),
            WalMsg::FullTxs(txs) => (
                get_column(&self.wal, W_FULLTXS)?,
                WAL_MAP
                    .get(W_FULLTXS)
                    .ok_or_else(|| ConsensusError::WalErr("Invalid key".to_string()))?,
                txs.encode()
                    .map_err(|e| ConsensusError::WalErr(format!("Encode full txs error {:?}", e)))?
                    .as_ref()
                    .to_vec(),
            ),
            WalMsg::QC(qc) => (
                get_column(&self.wal, W_QC)?,
                WAL_MAP
                    .get(W_QC)
                    .ok_or_else(|| ConsensusError::WalErr("Invalid key".to_string()))?,
                qc.rlp_bytes(),
            ),
        };

        self.wal
            .put_cf(column, key, value)
            .map_err(|e| ConsensusError::WalErr(e.to_string()))?;
        Ok(())
    }

    pub fn try_load<P: Codec, C: Codec>(&self) -> ConsensusResult<Option<WalStatus<P, C>>> {
        if self.epoch_id > INIT_EPOCH_ID {
            let status = self.load()?;
            return Ok(Some(status));
        }

        Ok(None)
    }

    fn load<P: Codec, C: Codec>(&self) -> ConsensusResult<WalStatus<P, C>> {
        // Load authority list.
        let tmp = self
            .get(W_VALIDATORS)?
            .ok_or_else(|| ConsensusError::WalErr("Missing authority list".to_string()))?;
        let authority: ValidatorList =
            decode(&tmp).map_err(|e| ConsensusError::WalErr(e.to_string()))?;

        // Load epoch ID.
        let tmp = self
            .get(W_EPOCH_ID)?
            .ok_or_else(|| ConsensusError::WalErr("Missing epoch ID".to_string()))?;
        let epoch_id: WalEpochId =
            decode(&tmp).map_err(|e| ConsensusError::WalErr(e.to_string()))?;

        // Load round.
        let round = if let Some(tmp) = self.get(W_ROUND)? {
            let wal_round: WalRound =
                decode(&tmp).map_err(|e| ConsensusError::WalErr(e.to_string()))?;
            wal_round
        } else {
            WalRound::new(INIT_ROUND)
        };

        // Load step.
        let step = if let Some(tmp) = self.get(W_STEP)? {
            let wal_step: WalStep =
                decode(&tmp).map_err(|e| ConsensusError::WalErr(e.to_string()))?;
            wal_step
        } else {
            WalStep::from(Step::default())
        };

        // Load proposal
        let proposal = if let Some(tmp) = self.get(W_PROPOSAL)? {
            let res: Proposal<P> =
                decode(&tmp).map_err(|e| ConsensusError::WalErr(e.to_string()))?;
            Some(res)
        } else {
            None
        };

        // Load full transcations
        let full_txs = if let Some(tmp) = self.get(W_FULLTXS)? {
            let txs = Codec::decode(Bytes::from(tmp))
                .map_err(|e| ConsensusError::WalErr(e.to_string()))?;
            Some(txs)
        } else {
            None
        };

        // Load quorum certificate
        let qc = if let Some(tmp) = self.get(W_QC)? {
            let res: AggregatedSignature =
                decode(&tmp).map_err(|e| ConsensusError::WalErr(e.to_string()))?;
            Some(res)
        } else {
            None
        };

        Ok(WalStatus {
            authority: authority.0,
            epoch_id: epoch_id.0,
            round: round.0,
            step: step.into(),
            proposal,
            full_txs,
            qc,
        })
    }

    pub fn clear_lock(&mut self) -> ConsensusResult<()> {
        self.remove(W_FULLTXS)?;
        self.remove(W_QC)?;
        Ok(())
    }

    fn get(&self, key: &str) -> ConsensusResult<Option<Vec<u8>>> {
        let column = get_column(&self.wal, key)?;
        let key = WAL_MAP
            .get(key)
            .ok_or_else(|| ConsensusError::WalErr("Invalid key".to_string()))?;
        let opt_bytes = self
            .wal
            .get_cf(column, key)
            .map_err(|e| ConsensusError::WalErr(e.to_string()))?;

        if let Some(db_vec) = opt_bytes {
            return Ok(Some(db_vec.to_vec()));
        }
        Ok(None)
    }

    fn clear_all(&mut self) -> ConsensusResult<()> {
        self.remove(W_EPOCH_ID)?;
        self.remove(W_VALIDATORS)?;
        self.remove(W_ROUND)?;
        self.remove(W_STEP)?;
        self.remove(W_PROPOSAL)?;
        self.remove(W_FULLTXS)?;
        self.remove(W_QC)?;
        Ok(())
    }

    fn remove(&mut self, key: &str) -> ConsensusResult<()> {
        let column = get_column(&self.wal, key)?;
        let key = WAL_MAP
            .get(key)
            .ok_or_else(|| ConsensusError::WalErr(format!("Invalid key {:?}", key)))?;

        self.wal
            .delete_cf(column, key)
            .map_err(|e| ConsensusError::WalErr(e.to_string()))?;
        Ok(())
    }
}

fn get_column<'a>(db: &'a DB, category: &'a str) -> ConsensusResult<ColumnFamily<'a>> {
    let column = db
        .cf_handle(category)
        .ok_or_else(|| ConsensusError::WalErr(format!("Rocks get {:?} column error", category)))?;

    Ok(column)
}

#[cfg(test)]
mod test {
    extern crate test;

    use std::error::Error;

    use bincode::{deserialize, serialize};
    use rand::random;
    use serde::{Deserialize, Serialize};
    use test::Bencher;

    use crate::types::{AggregatedSignature, Node, Proposal};

    use super::*;

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
    struct Pill {
        epoch_id: u64,
        epoch:    Vec<u64>,
    }

    impl Codec for Pill {
        fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
            let encode: Vec<u8> = serialize(&self).expect("Serialize Pill error");
            Ok(Bytes::from(encode))
        }

        fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>> {
            let decode: Pill = deserialize(&data.as_ref()).expect("Deserialize Pill error");
            Ok(decode)
        }
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
    struct Transcations(Vec<Vec<u8>>);

    impl Codec for Transcations {
        fn encode(&self) -> Result<Bytes, Box<dyn Error + Send>> {
            let encode: Vec<u8> = serialize(&self).expect("Serialize Transcations error");
            Ok(Bytes::from(encode))
        }

        fn decode(data: Bytes) -> Result<Self, Box<dyn Error + Send>> {
            let decode: Transcations =
                deserialize(data.as_ref()).expect("Deserialize Transcations error");
            Ok(decode)
        }
    }

    fn gen_auth_list() -> Vec<Node> {
        let node = Node {
            address:        Bytes::from(vec![0u8]),
            propose_weight: random::<u8>(),
            vote_weight:    random::<u8>(),
        };

        vec![node]
    }

    fn gen_pill() -> Pill {
        Pill {
            epoch_id: random::<u64>(),
            epoch:    (0..256).map(|_| random::<u64>()).collect::<Vec<_>>(),
        }
    }

    fn gen_transactions() -> Transcations {
        Transcations(vec![(0..1024 * 1024)
            .map(|_| random::<u8>())
            .collect::<Vec<_>>()])
    }

    fn gen_proposal(pill: Pill) -> Proposal<Pill> {
        Proposal {
            epoch_id:   gen_epoch_id(),
            round:      gen_round(),
            content:    pill,
            epoch_hash: Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
            lock:       None,
            proposer:   Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
        }
    }

    fn gen_aggr_signature() -> AggregatedSignature {
        AggregatedSignature {
            signature:      Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
            address_bitmap: Bytes::from((0..32).map(|_| random::<u8>()).collect::<Vec<_>>()),
        }
    }

    fn gen_epoch_id() -> u64 {
        random::<u64>()
    }

    fn gen_round() -> u64 {
        random::<u64>()
    }

    #[test]
    fn test_wal() {
        let mut wal = Wal::new("./logs/wal").expect("Create wal failed");
        let validators = gen_auth_list();
        let epoch_id = gen_epoch_id();
        let round = gen_round();
        let pill = gen_pill();
        let proposal = gen_proposal(pill.clone());
        let qc = gen_aggr_signature();
        let full_txs = gen_transactions();

        wal.save::<Pill, Transcations>(WalMsg::EpochId(epoch_id))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::Authority(validators.clone()))
            .unwrap();
        let status = wal.load::<Pill, Transcations>().unwrap();

        assert_eq!(status.authority, validators);
        assert_eq!(status.epoch_id, epoch_id);
        assert_eq!(status.round, INIT_ROUND);
        assert!(status.proposal.is_none());
        assert!(status.qc.is_none());

        wal.save::<Pill, Transcations>(WalMsg::Round(round))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::Step(Step::Precommit))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::Proposal(proposal.clone()))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::FullTxs(full_txs.clone()))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::QC(qc.clone()))
            .unwrap();
        let status = wal.load::<Pill, Transcations>().unwrap();

        assert_eq!(status.round, round);
        assert_eq!(status.step, Step::Precommit);
        assert_eq!(status.proposal, Some(proposal));
        assert_eq!(status.full_txs, Some(full_txs));
        assert_eq!(status.qc, Some(qc));

        let new_validators = gen_auth_list();
        let mut new_epoch_id = gen_epoch_id();
        while new_epoch_id <= epoch_id {
            new_epoch_id = gen_epoch_id();
        }

        wal.save::<Pill, Transcations>(WalMsg::EpochId(new_epoch_id))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::Authority(new_validators.clone()))
            .unwrap();
        let status = wal.load::<Pill, Transcations>().unwrap();

        assert_eq!(status.authority, new_validators);
        assert_eq!(status.epoch_id, new_epoch_id);
        assert_eq!(status.round, INIT_ROUND);
        assert!(status.proposal.is_none());
        assert!(status.qc.is_none());
    }

    #[bench]
    fn bench_save_txs(b: &mut Bencher) {
        let mut wal = Wal::new("./logs/wal_2").expect("Create wal failed");
        let validators = gen_auth_list();
        let epoch_id = gen_epoch_id();
        let round = gen_round();
        let pill = gen_pill();
        let proposal = gen_proposal(pill.clone());
        let full_txs = gen_transactions();

        wal.save::<Pill, Transcations>(WalMsg::EpochId(epoch_id))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::Authority(validators.clone()))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::Round(round))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::Step(Step::Precommit))
            .unwrap();
        wal.save::<Pill, Transcations>(WalMsg::Proposal(proposal))
            .unwrap();

        b.iter(|| {
            wal.save::<Pill, Transcations>(WalMsg::FullTxs(full_txs.clone()))
                .unwrap()
        });
    }
}
