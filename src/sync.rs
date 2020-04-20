use std::collections::HashSet;
use std::marker::PhantomData;

use derive_more::Display;
use log::info;

use crate::timeout::TimeoutEvent;
use crate::types::{DisplayVec, OverlordMsg, SignedHeight, SyncRequest, SyncResponse, TinyHex};
use crate::{Address, Blk, OverlordError, OverlordResult};

pub const HEIGHT_RATIO: u64 = 10;
/// timeout waiting for a sync request.
/// sync_timeout = `SYNC_TIMEOUT_RATIO`/`TIME_DIVISOR` * `interval`
pub const SYNC_TIMEOUT_RATIO: u64 = 10;
/// timeout for removing one address in black list
/// clear_timeout = `CLEAR_TIMEOUT_RATIO`/`TIME_DIVISOR` * `interval`
pub const CLEAR_TIMEOUT_RATIO: u64 = 40;
/// max number of blocks can be request in one request
pub const BLOCK_BATCH: u64 = 50;

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
    fmt = "{{ state: {}, black_list: {} }}",
    state,
    "DisplayVec::<String>::new(&self.black_list.iter().map(|ad| ad.tiny_hex()).collect::<Vec<String>>())"
)]
pub struct Sync<B: Blk> {
    pub address: Address,
    pub state: SyncStat,
    pub request_id: u64,
    /// To void DoS Attack, One will request/response one address every clear_timeout cycle
    pub black_list: HashSet<Address>,
    pub phantom: PhantomData<B>,
}

impl<B: Blk> Sync<B> {
    pub fn new(address: Address) -> Self {
        Sync {
            address,
            state: SyncStat::Off,
            request_id: 0,
            black_list: HashSet::new(),
            phantom: PhantomData,
        }
    }

    pub fn handle_sync_request(&mut self, request: &SyncRequest) -> OverlordResult<()> {
        if self.state == SyncStat::On {
            return Err(OverlordError::net_on_sync());
        }
        if self.black_list.contains(&request.requester) {
            return Err(OverlordError::net_blacklist());
        }
        let old_sync = self.clone();
        self.black_list.insert(request.requester.clone());
        self.log_state_update_of_msg(old_sync, &request.requester, request.clone().into());
        Ok(())
    }

    pub fn handle_signed_height(&mut self, signed_height: &SignedHeight) -> OverlordResult<()> {
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
        self.log_state_update_of_msg(
            old_sync,
            &signed_height.address,
            signed_height.clone().into(),
        );

        Ok(())
    }

    pub fn handle_sync_response(&mut self, response: &SyncResponse<B>) {
        let old_sync = self.clone();
        self.state = SyncStat::Off;
        self.log_state_update_of_msg(old_sync, &response.responder, response.clone().into());
    }

    pub fn handle_sync_timeout(&mut self, request_id: u64) -> OverlordResult<()> {
        if request_id < self.request_id {
            return Err(OverlordError::debug_old());
        }
        let old_sync = self.clone();
        self.state = SyncStat::Off;
        self.log_state_update_of_timeout(old_sync, TimeoutEvent::SyncTimeout(request_id));
        Ok(())
    }

    pub fn handle_clear_timeout(&mut self, address: &Address) {
        let old_sync = self.clone();
        self.black_list.remove(address);
        self.log_state_update_of_timeout(old_sync, TimeoutEvent::ClearTimeout(address.clone()));
    }

    fn log_state_update_of_msg(&self, old_sync: Sync<B>, from: &Address, msg: OverlordMsg<B>) {
        info!(
            "[RECEIVE]\n\t<{}> <- {}\n\t<message> {}\n\t<before> sync: {}\n\t<after> sync: {}\n",
            self.address.tiny_hex(),
            from.tiny_hex(),
            msg,
            old_sync,
            self
        );
    }

    fn log_state_update_of_timeout(&self, old_sync: Sync<B>, timeout: TimeoutEvent) {
        info!(
            "[TIMEOUT]\n\t<{}> <- timeout\n\t<timeout> {} \n\t<before> state: {} \n\t<update> state: {}\n",
            self.address.tiny_hex(),
            timeout,
            old_sync,
            self
        );
    }
}
