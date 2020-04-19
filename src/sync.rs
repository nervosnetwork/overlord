#![allow(unused_imports)]

use std::collections::HashSet;

use derive_more::Display;

use crate::types::{DisplayVec, SignedHeight, SyncRequest, SyncResponse, TinyHex};
use crate::{Address, Blk, Hash, HeightRange, OverlordError, OverlordResult};

pub const HEIGHT_RATIO: u64 = 10;
/// timeout waiting for a sync request.
/// sync_timeout = `SYNC_TIMEOUT_RATIO`/`TIME_DIVISOR` * `interval`
pub const SYNC_TIMEOUT_RATIO: u64 = 10;
/// timeout for removing one address in black list
/// clear_timeout = `CLEAR_TIMEOUT_RATIO`/`TIME_DIVISOR` * `interval`
pub const CLEAR_TIMEOUT_RATIO: u64 = 40;
/// max number of blocks can be request in one request
pub const BLOCK_BATCH: u64 = 50;

#[derive(Debug, Display, Eq, PartialEq)]
pub enum SyncStat {
    #[display(fmt = "on")]
    On,
    #[display(fmt = "off")]
    Off,
}

impl Default for SyncStat {
    fn default() -> Self {
        SyncStat::Off
    }
}

#[derive(Debug, Display, Default)]
#[display(
    fmt = "{{ state: {}, black_list: {} }}",
    state,
    "DisplayVec::<String>::new(&self.black_list.iter().map(|ad| ad.tiny_hex()).collect::<Vec<String>>())"
)]
pub struct Sync {
    pub state: SyncStat,
    pub request_id: u64,
    /// To void DoS Attack, One will request/response one address every clear_timeout cycle
    pub black_list: HashSet<Address>,
}

impl Sync {
    pub fn handle_sync_request(&mut self, request: &SyncRequest) -> OverlordResult<()> {
        if self.state == SyncStat::On {
            return Err(OverlordError::net_on_sync());
        }
        if self.black_list.contains(&request.requester) {
            return Err(OverlordError::net_blacklist());
        }
        self.black_list.insert(request.requester.clone());
        Ok(())
    }

    pub fn handle_signed_height(&mut self, signed_height: &SignedHeight) -> OverlordResult<()> {
        if self.state == SyncStat::On {
            return Err(OverlordError::debug_on_sync());
        }
        if self.black_list.contains(&signed_height.address) {
            return Err(OverlordError::net_blacklist());
        }
        self.state = SyncStat::On;
        self.request_id += 1;
        self.black_list.insert(signed_height.address.clone());
        Ok(())
    }

    pub fn handle_sync_response<B: Blk>(&mut self) {
        self.state = SyncStat::Off;
    }

    pub fn handle_sync_timeout(&mut self, request_id: u64) -> OverlordResult<()> {
        if request_id < self.request_id {
            return Err(OverlordError::debug_old());
        }
        self.state = SyncStat::Off;
        Ok(())
    }

    pub fn handle_clear_timeout(&mut self, address: &Address) {
        self.black_list.remove(address);
    }
}
