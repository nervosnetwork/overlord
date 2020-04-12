use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bytes::Bytes;
use derive_more::Display;

const WAL_FILE_NAME: &str = "state.wal";

/// Simple Write Ahead Logging
#[derive(Debug)]
pub struct Wal {
    wal_dir_path: PathBuf,
}

impl Wal {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Wal {
            wal_dir_path: path.as_ref().to_path_buf(),
        }
    }

    pub fn save(&self, info: Bytes) -> Result<(), Box<dyn Error + Send>> {
        let mut wal_file = self.safe_open_file()?;

        wal_file
            .write_all(info.as_ref())
            .map_err(WalError::WriteFailed)?;
        Ok(())
    }

    pub fn load(&self) -> Result<Bytes, Box<dyn Error + Send>> {
        let mut wal_file = self.safe_open_file()?;

        let mut read_buf = Vec::new();
        let _ = wal_file
            .read_to_end(&mut read_buf)
            .map_err(WalError::ReadFailed)?;
        Ok(Bytes::from(read_buf))
    }

    fn safe_open_file(&self) -> Result<fs::File, WalError> {
        if !self.wal_dir_path.exists() {
            fs::create_dir_all(&self.wal_dir_path).expect("Create wal directory failed");
        }
        let mut wal_file_path = self.wal_dir_path.clone();
        wal_file_path.push(WAL_FILE_NAME.to_owned());

        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&wal_file_path)
            .map_err(WalError::OpenFileFailed)
    }
}

#[derive(Debug, Display)]
pub enum WalError {
    #[display(fmt = "Open wal file failed, {:?}", _0)]
    OpenFileFailed(std::io::Error),

    #[display(fmt = "Read wal file failed, {:?}", _0)]
    ReadFailed(std::io::Error),

    #[display(fmt = "Write wal file failed, {:?}", _0)]
    WriteFailed(std::io::Error),
}

impl From<WalError> for Box<dyn Error + Send> {
    fn from(error: WalError) -> Self {
        Box::new(error) as Box<dyn Error + Send>
    }
}

impl Error for WalError {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_default_wal() {
        let wal = Wal::new("./");
        let info = vec![0u8, 12u8, 31u8, 2u8, 19u8, 90u8, 113u8];
        let save_info = Bytes::from(info);
        println!("read empty wal file: {:?}", wal.load().unwrap());
        wal.save(save_info.clone()).unwrap();
        let load_info = wal.load().unwrap();
        assert_eq!(save_info, load_info);
    }
}
