#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::{Error, ErrorKind, SeekFrom};
use std::mem::transmute;

use log::{trace, warn};
use tokio::fs::{read_dir, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const DELETE_FILE_INTERVAL: u64 = 3;

/// A Wal struct to read and write *write-aheads-log*.
#[derive(Debug)]
pub struct Wal {
    path:          String,
    current_epoch: u64,
    file_map:      BTreeMap<u64, File>,
    ifile:         File,
}

impl Wal {
    pub async fn new(dir: String) -> Result<Wal, Error> {
        let _ = read_dir(dir.clone()).await?;

        let wal_path = dir.clone() + "/" + "index";
        let mut current_file = OpenOptions::new();
        let mut file = current_file
            .read(true)
            .create(true)
            .write(true)
            .open(wal_path.clone())
            .await?;
        file.seek(SeekFrom::Start(0))
            .await
            .unwrap_or_else(|_| panic!("Seek wal file {:?} failed!", &wal_path));

        let mut buf = String::new();
        let (cur_epoch, last_file_path) = if file.read_to_string(&mut buf).await? == 0 {
            (0u64, dir.clone() + "/0.log")
        } else if let Ok(epoch) = buf.parse::<u64>() {
            (
                epoch,
                dir.clone() + "/" + epoch.to_string().as_str() + ".log",
            )
        } else {
            return Err(Error::new(ErrorKind::InvalidData, "index file data wrong"));
        };

        let mut file_system = OpenOptions::new();
        let fs = file_system
            .read(true)
            .create(true)
            .write(true)
            .open(last_file_path)
            .await?;
        let mut map = BTreeMap::new();
        map.insert(cur_epoch, fs);

        Ok(Wal {
            path:          dir,
            current_epoch: cur_epoch,
            file_map:      map,
            ifile:         file,
        })
    }

    pub async fn set_height(&mut self, height: u64) -> Result<(), Error> {
        self.current_epoch = height;
        self.ifile.seek(SeekFrom::Start(0)).await?;
        let hstr = height.to_string();
        let content = hstr.as_bytes();
        let _ = self.ifile.set_len(content.len() as u64);
        self.ifile.write_all(&content).await?;
        self.ifile.sync_data().await?;

        let filename = Wal::get_file_path(&self.path, height);
        let mut fs = OpenOptions::new();
        let fs = fs
            .create(true)
            .read(true)
            .write(true)
            .open(filename)
            .await?;
        self.file_map.insert(height, fs);

        if height > DELETE_FILE_INTERVAL {
            let saved_epoch_fs = self.file_map.split_off(&(height - DELETE_FILE_INTERVAL));
            {
                for (height, _) in self.file_map.iter() {
                    let delfilename = Wal::get_file_path(&self.path, *height);
                    let _ = ::std::fs::remove_file(delfilename);
                }
            }
            self.file_map = saved_epoch_fs;
        }
        Ok(())
    }

    pub async fn save(&mut self, height: u64, mtype: u8, msg: String) -> Result<u64, Error> {
        trace!("Wal save mtype: {}, height: {}", mtype, height);
        if !self.file_map.contains_key(&height) {
            // 2 more higher than current height, do not process it
            if height > self.current_epoch + 1 {
                return Ok(0);
            } else if height == self.current_epoch + 1 {
                let filename = Wal::get_file_path(&self.path, height);
                let mut fs = OpenOptions::new();
                let fs = fs
                    .read(true)
                    .create(true)
                    .write(true)
                    .open(filename)
                    .await?;
                self.file_map.insert(height, fs);
            }
        }

        if msg.is_empty() {
            return Ok(0);
        }

        let mlen = msg.len() as u32;
        let mut hlen = 0;
        if let Some(fs) = self.file_map.get_mut(&height) {
            let len_bytes: [u8; 4] = unsafe { transmute(mlen.to_le()) };
            let type_bytes: [u8; 1] = unsafe { transmute(mtype.to_le()) };
            fs.seek(SeekFrom::End(0)).await?;
            fs.write_all(&len_bytes[..]).await?;
            fs.write_all(&type_bytes[..]).await?;
            hlen = fs.write(msg.as_bytes()).await?;
            fs.flush().await?;
        } else {
            warn!("Can't find wal log in height {} ", height);
        }
        Ok(hlen as u64)
    }

    pub async fn load(&mut self) -> Vec<(u8, Vec<u8>)> {
        let mut vec_buf: Vec<u8> = Vec::new();
        let mut vec_out: Vec<(u8, Vec<u8>)> = Vec::new();
        let cur_height = self.current_epoch;
        if self.file_map.is_empty() || cur_height == 0 {
            return vec_out;
        }

        for (height, fs) in self.file_map.iter() {
            if *height < self.current_epoch {
                continue;
            }
            let expect_str = format!("Seek wal file {:?} of height {} failed!", fs, height);
            let mut file_system = fs.try_clone().await.expect(&expect_str);
            file_system
                .seek(SeekFrom::Start(0))
                .await
                .expect(&expect_str);
            let res_fsize = file_system.read_to_end(&mut vec_buf).await;
            if res_fsize.is_err() {
                return vec_out;
            }

            let expect_str = format!(
                "Get size of buf of wal file {:?} of height {} failed!",
                fs, height
            );
            let fsize = res_fsize.expect(&expect_str);
            if fsize <= 5 {
                return vec_out;
            }

            let mut index = 0;
            loop {
                if index + 5 > fsize {
                    break;
                }
                let hd: [u8; 4] = [
                    vec_buf[index],
                    vec_buf[index + 1],
                    vec_buf[index + 2],
                    vec_buf[index + 3],
                ];
                let tmp: u32 = unsafe { transmute::<[u8; 4], u32>(hd) };
                let bodylen = tmp as usize;
                let mtype = vec_buf[index + 4];
                index += 5;
                if index + bodylen > fsize {
                    break;
                }
                vec_out.push((mtype, vec_buf[index..index + bodylen].to_vec()));
                index += bodylen;
            }
        }
        vec_out
    }

    fn get_file_path(dir: &str, height: u64) -> String {
        let mut name = height.to_string();
        name += ".log";
        let pathname = dir.to_string() + "/";
        pathname.clone() + &*name
    }
}
