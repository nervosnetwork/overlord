use std::collections::HashSet;
use std::marker::PhantomData;

use derive_more::Display;

use crate::types::{DisplayVec, SignedHeight, SyncRequest, TinyHex};
use crate::{Address, Blk, OverlordError, OverlordResult};

pub const HEIGHT_RATIO: u64 = 11;
/// timeout waiting for a sync request.
/// sync_timeout = `SYNC_TIMEOUT_RATIO`/`TIME_DIVISOR` * `interval`
pub const SYNC_TIMEOUT_RATIO: u64 = 23;
/// timeout for removing one address in black list
/// clear_timeout = `CLEAR_TIMEOUT_RATIO`/`TIME_DIVISOR` * `interval`
pub const CLEAR_TIMEOUT_RATIO: u64 = 29;
/// max number of blocks can be request in one request
pub const BLOCK_BATCH: u64 = 10;

#[derive(Clone, Debug, Display, Eq, PartialEq)]
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

#[derive(Clone, Debug, Display, Default)]
#[display(
    fmt = "{{ state: {}, request_id: {}, black_list: {} }}",
    state,
    request_id,
    "DisplayVec::<String>::new(&self.black_list.iter().map(|ad| ad.tiny_hex()).collect::<Vec<String>>())"
)]
pub struct Sync<B: Blk> {
    pub state: SyncStat,
    pub request_id: u64,
    /// To void DoS Attack, One will request/response one address every clear_timeout cycle
    pub black_list: HashSet<Address>,

    phantom: PhantomData<B>,
}

impl<B: Blk> Sync<B> {
    pub fn new() -> Self {
        Sync {
            state:      SyncStat::Off,
            request_id: 0,
            black_list: HashSet::new(),
            phantom:    PhantomData,
        }
    }

    pub fn handle_sync_request(&mut self, request: &SyncRequest) -> OverlordResult<Sync<B>> {
        if self.state == SyncStat::On {
            return Err(OverlordError::net_on_sync());
        }
        if self.black_list.contains(&request.requester) {
            return Err(OverlordError::net_blacklist());
        }
        let old_sync = self.clone();
        self.black_list.insert(request.requester.clone());
        Ok(old_sync)
    }

    pub fn handle_signed_height(
        &mut self,
        signed_height: &SignedHeight,
    ) -> OverlordResult<Sync<B>> {
        if self.state == SyncStat::On {
            return Err(OverlordError::debug_on_sync());
        }
        if self.black_list.contains(&signed_height.address) {
            return Err(OverlordError::net_blacklist());
        }
        let old_sync = self.clone();
        self.state = SyncStat::On;
        self.request_id += 1;
        self.black_list.insert(signed_height.address.clone());
        Ok(old_sync)
    }

    pub fn handle_sync_response(&mut self) -> Sync<B> {
        let old_sync = self.clone();
        self.turn_off_sync();
        old_sync
    }

    pub fn handle_sync_timeout(&mut self, request_id: u64) -> OverlordResult<Sync<B>> {
        if request_id < self.request_id {
            return Err(OverlordError::debug_old());
        }
        let old_sync = self.clone();
        self.turn_off_sync();
        Ok(old_sync)
    }

    pub fn handle_clear_timeout(&mut self, address: &Address) -> Sync<B> {
        let old_sync = self.clone();
        self.black_list.remove(address);
        old_sync
    }

    pub fn turn_off_sync(&mut self) {
        self.state = SyncStat::Off;
    }
}
