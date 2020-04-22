use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bytes::Bytes;

use crate::state::StateInfo;
use crate::types::FetchedFullBlock;
use crate::{Blk, Height, OverlordError, OverlordResult, TinyHex};

const STATE_SUB_DIR: &str = "state";
const STATE_FILE_NAME: &str = "state.wal";
const FULL_BLOCK_SUB_DIR: &str = "full_block";

/// Simple Write Ahead Logging
#[derive(Debug)]
pub struct Wal {
    pub wal_dir_path:   PathBuf,
    pub state_dir_path: PathBuf,
}

impl Wal {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let wal_dir_path = path.as_ref().to_path_buf();
        let mut state_dir_path = wal_dir_path.clone();
        state_dir_path.push(STATE_SUB_DIR);
        Wal {
            wal_dir_path,
            state_dir_path,
        }
    }

    pub fn save_state<B: Blk>(&self, state: &StateInfo<B>) -> OverlordResult<()> {
        let encode = rlp::encode(state);
        self.safe_save_file(
            self.state_dir_path.clone(),
            STATE_FILE_NAME.to_owned(),
            &encode,
        )
    }

    pub fn load_state<B: Blk>(&self) -> OverlordResult<StateInfo<B>> {
        let encode =
            self.safe_load_file(self.state_dir_path.clone(), STATE_FILE_NAME.to_owned())?;
        rlp::decode(&encode).map_err(OverlordError::local_decode)
    }

    pub fn save_full_block(&self, fetch: &FetchedFullBlock) -> OverlordResult<()> {
        let dir = self.assemble_full_block_dir(fetch.height);
        let file_name = fetch.block_hash.tiny_hex() + ".wal";
        self.safe_save_file(dir, file_name, &rlp::encode(fetch))
    }

    pub fn load_full_blocks(&self) -> OverlordResult<Vec<FetchedFullBlock>> {
        let mut vec = vec![];

        let mut full_block_path = self.wal_dir_path.clone();
        full_block_path.push(FULL_BLOCK_SUB_DIR);
        ensure_dir_exists(&full_block_path);

        for dir_entry in fs::read_dir(full_block_path).map_err(OverlordError::local_wal)? {
            let folder = dir_entry.map_err(OverlordError::local_wal)?.path();
            for file_entry in fs::read_dir(folder).map_err(OverlordError::local_wal)? {
                let file_path = file_entry.map_err(OverlordError::local_wal)?.path();
                let mut file = open_file(file_path)?;
                let mut read_buf = Vec::new();
                let _ = file
                    .read_to_end(&mut read_buf)
                    .map_err(OverlordError::local_wal)?;
                let fetch: FetchedFullBlock =
                    rlp::decode(&read_buf).map_err(OverlordError::local_decode)?;
                vec.push(fetch);
            }
        }
        Ok(vec)
    }

    pub fn handle_commit(&self, height: Height) -> OverlordResult<()> {
        self.remove_full_blocks(height)
    }

    fn remove_full_blocks(&self, height: Height) -> OverlordResult<()> {
        let mut full_block_path = self.wal_dir_path.clone();
        full_block_path.push(FULL_BLOCK_SUB_DIR);

        ensure_dir_exists(&full_block_path);

        for entry in fs::read_dir(full_block_path).map_err(OverlordError::local_wal)? {
            let folder = entry.map_err(OverlordError::local_wal)?.path();
            let folder_name = folder
                .file_stem()
                .ok_or_else(|| OverlordError::local_other("file stem error".to_owned()))?
                .to_os_string()
                .clone();
            let folder_name = folder_name.into_string().map_err(|err| {
                OverlordError::local_other(format!("transfer os string to string error {:?}", err))
            })?;
            let height_of_folder = folder_name.parse::<u64>().map_err(|err| {
                OverlordError::local_other(format!(
                    "parse folder name {:?} error {:?}",
                    folder, err
                ))
            })?;

            if height_of_folder < height {
                fs::remove_dir_all(folder).map_err(OverlordError::local_wal)?;
            }
        }

        Ok(())
    }

    fn safe_open_file(&self, dir: PathBuf, file_name: String) -> OverlordResult<fs::File> {
        ensure_dir_exists(&dir);

        let mut wal_file_path = dir;
        wal_file_path.push(file_name);

        open_file(wal_file_path)
    }

    fn safe_save_file(&self, dir: PathBuf, file_name: String, data: &[u8]) -> OverlordResult<()> {
        let mut wal_file = self.safe_open_file(dir, file_name)?;

        wal_file.write_all(data).map_err(OverlordError::local_wal)?;
        Ok(())
    }

    fn safe_load_file(&self, dir: PathBuf, file_name: String) -> OverlordResult<Bytes> {
        let mut wal_file = self.safe_open_file(dir, file_name)?;

        let mut read_buf = Vec::new();
        let _ = wal_file
            .read_to_end(&mut read_buf)
            .map_err(OverlordError::local_wal)?;
        Ok(Bytes::from(read_buf))
    }

    fn assemble_full_block_dir(&self, height: Height) -> PathBuf {
        let mut full_block_dir = self.wal_dir_path.clone();
        full_block_dir.push(FULL_BLOCK_SUB_DIR);
        full_block_dir.push(height.to_string());
        full_block_dir
    }
}

fn ensure_dir_exists(dir: &PathBuf) {
    if !dir.exists() {
        fs::create_dir_all(dir)
            .expect("Failed to create wal directory! It's meaningless to continue running");
    }
}

fn open_file(file_path: PathBuf) -> OverlordResult<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&file_path)
        .map_err(OverlordError::local_wal)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::state::Stage;
    use crate::types::{
        Aggregates, Choke, ChokeQC, PreCommitQC, PreVoteQC, TestBlock, UpdateFrom, Vote,
    };
    use crate::{Crypto, DefaultCrypto};
    use rand::random;

    #[test]
    fn test_wal() {
        let wal = Wal::new("./wal/");

        let aggregates = Aggregates {
            address_bitmap: Bytes::from("iewuceiu"),
            signature:      Bytes::from("iewuceiu"),
        };

        let vote = Vote::new(10, 2, Bytes::from("ckvfkvfv"));

        let state = StateInfo {
            stage:         Stage::default(),
            lock:          Some(PreVoteQC::new(vote.clone(), aggregates.clone())),
            block:         Some(TestBlock::default()),
            pre_commit_qc: Some(PreCommitQC::new(vote, aggregates.clone())),
            from:          Some(UpdateFrom::ChokeQC(ChokeQC::new(
                Choke::default(),
                aggregates,
            ))),
        };

        wal.save_state(&state).unwrap();
        let load_state = wal.load_state().expect("load state failed");
        assert_eq!(state, load_state);

        let full_block = Bytes::from(gen_random_bytes(1000));
        let hash = DefaultCrypto::hash(&full_block);
        let fetch = FetchedFullBlock::new(10, hash.clone(), full_block.clone());
        wal.save_full_block(&fetch).unwrap();
        let fetches = wal.load_full_blocks().expect("load full blocks failed");
        assert_eq!(fetch, fetches[0]);

        wal.save_full_block(&FetchedFullBlock::new(11, hash, full_block))
            .unwrap();
        wal.remove_full_blocks(12)
            .expect("remove full blocks failed");
    }

    fn gen_random_bytes(len: usize) -> Vec<u8> {
        (0..len).map(|_| random::<u8>()).collect::<Vec<_>>()
    }
}
