use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bytes::Bytes;
use derive_more::Display;

use crate::{Hash, Height};

#[allow(dead_code)]
const STATE_SUB_DIR: &str = "state";
#[allow(dead_code)]
const STATE_FILE_NAME: &str = "state.wal";
#[allow(dead_code)]
const FULL_BLOCK_SUB_DIR: &str = "full_block";

/// Simple Write Ahead Logging
#[derive(Debug)]
pub struct Wal {
    wal_dir_path:   PathBuf,
    state_dir_path: PathBuf,
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

    #[allow(dead_code)]
    pub fn save_state(&self, state: &Bytes) -> Result<(), WalError> {
        self.safe_save_file(
            self.state_dir_path.clone(),
            STATE_FILE_NAME.to_owned(),
            state,
        )
    }

    #[allow(dead_code)]
    pub fn load_state(&self) -> Result<Bytes, WalError> {
        self.safe_load_file(self.state_dir_path.clone(), STATE_FILE_NAME.to_owned())
    }

    #[allow(dead_code)]
    pub fn save_full_block(
        &self,
        height: Height,
        block_hash: &Hash,
        full_block: &Bytes,
    ) -> Result<(), WalError> {
        let dir = self.assemble_full_block_dir(height);
        let file_name = hex::encode(block_hash) + ".wal";
        self.safe_save_file(dir, file_name, full_block)
    }

    #[allow(dead_code)]
    pub fn load_full_block(&self, height: Height, block_hash: &Hash) -> Result<Bytes, WalError> {
        let dir = self.assemble_full_block_dir(height);
        let file_name = hex::encode(block_hash) + ".wal";
        self.safe_load_file(dir, file_name)
    }

    #[allow(dead_code)]
    pub fn remove_full_blocks(&self, height: Height) -> Result<(), WalError> {
        let mut full_block_path = self.wal_dir_path.clone();
        full_block_path.push(FULL_BLOCK_SUB_DIR);

        ensure_dir_exists(&full_block_path);

        for entry in fs::read_dir(full_block_path).map_err(WalError::ReadDirFailed)? {
            let folder = entry.map_err(WalError::ReadDirFailed)?.path();
            let folder_name = folder
                .file_stem()
                .ok_or_else(|| WalError::Other("file stem error".to_string()))?
                .to_os_string()
                .clone();
            let folder_name = folder_name.into_string().map_err(|err| {
                WalError::Other(format!("transfer os string to string error {:?}", err))
            })?;
            let height_of_folder = folder_name.parse::<u64>().map_err(|err| {
                WalError::Other(format!("parse folder name {:?} error {:?}", folder, err))
            })?;

            if height_of_folder <= height {
                fs::remove_dir_all(folder).map_err(WalError::RemoveDirFailed)?;
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn safe_open_file(&self, dir: PathBuf, file_name: String) -> Result<fs::File, WalError> {
        ensure_dir_exists(&dir);

        let mut wal_file_path = dir;
        wal_file_path.push(file_name);

        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&wal_file_path)
            .map_err(WalError::OpenFileFailed)
    }

    #[allow(dead_code)]
    fn safe_save_file(&self, dir: PathBuf, file_name: String, data: &[u8]) -> Result<(), WalError> {
        let mut wal_file = self.safe_open_file(dir, file_name)?;

        wal_file
            .write_all(data)
            .map_err(WalError::WriteFileFailed)?;
        Ok(())
    }

    #[allow(dead_code)]
    fn safe_load_file(&self, dir: PathBuf, file_name: String) -> Result<Bytes, WalError> {
        let mut wal_file = self.safe_open_file(dir, file_name)?;

        let mut read_buf = Vec::new();
        let _ = wal_file
            .read_to_end(&mut read_buf)
            .map_err(WalError::ReadFileFailed)?;
        Ok(Bytes::from(read_buf))
    }

    #[allow(dead_code)]
    fn assemble_full_block_dir(&self, height: Height) -> PathBuf {
        let mut full_block_dir = self.wal_dir_path.clone();
        full_block_dir.push(FULL_BLOCK_SUB_DIR);
        full_block_dir.push(height.to_string());
        full_block_dir
    }
}

#[allow(dead_code)]
fn ensure_dir_exists(dir: &PathBuf) {
    if !dir.exists() {
        fs::create_dir_all(dir).expect("Create wal directory failed");
    }
}

#[allow(dead_code)]
#[derive(Debug, Display)]
pub enum WalError {
    #[display(fmt = "Open wal file failed, {:?}", _0)]
    OpenFileFailed(std::io::Error),

    #[display(fmt = "Read wal file failed, {:?}", _0)]
    ReadFileFailed(std::io::Error),

    #[display(fmt = "Write wal file failed, {:?}", _0)]
    WriteFileFailed(std::io::Error),

    #[display(fmt = "Read wal dir failed, {:?}", _0)]
    ReadDirFailed(std::io::Error),

    #[display(fmt = "Remove wal dir failed, {:?}", _0)]
    RemoveDirFailed(std::io::Error),

    #[display(fmt = "Other error: {}", _0)]
    Other(String),
}

impl Error for WalError {}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Crypto, DefaultCrypto};
    use rand::random;

    #[test]
    fn test_default_wal() {
        let wal = Wal::new("./wal/");
        let state = Bytes::from(gen_random_bytes(100));
        wal.save_state(&state).unwrap();
        let load_state = wal.load_state().unwrap();
        assert_eq!(state, load_state);

        let full_block = Bytes::from(gen_random_bytes(1000));
        let hash = DefaultCrypto::hash(&full_block);
        wal.save_full_block(10, &hash, &full_block).unwrap();
        let load_full_block = wal.load_full_block(10, &hash).unwrap();
        assert_eq!(full_block, load_full_block);

        wal.save_full_block(11, &hash, &full_block).unwrap();
        wal.remove_full_blocks(10).unwrap();
    }

    fn gen_random_bytes(len: usize) -> Vec<u8> {
        (0..len).map(|_| random::<u8>()).collect::<Vec<_>>()
    }
}
