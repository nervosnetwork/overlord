use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use log::info;

use crate::error::ErrorInfo;
use crate::exec::ExecRequest;
use crate::smr::WrappedOverlordMsg;
use crate::state::{Stage, Step};
use crate::sync::{CLEAR_TIMEOUT_RATIO, HEIGHT_RATIO, SYNC_TIMEOUT_RATIO};
use crate::timeout::{TimeoutEvent, TimeoutInfo};
use crate::types::FetchedFullBlock;
use crate::{
    Adapter, Address, Blk, ExecResult, Hash, OverlordError, OverlordMsg, OverlordResult, St,
    TimeConfig, TinyHex,
};

const POWER_CAP: u32 = 5;
const TIME_DIVISOR: u64 = 10;

pub struct EventAgent<A: Adapter<B, S>, B: Blk, S: St> {
    address:     Address,
    adapter:     Arc<A>,
    time_config: TimeConfig,
    start_time:  Instant, // start time of current height
    fetch_set:   HashSet<Hash>,

    pub from_net: UnboundedReceiver<WrappedOverlordMsg<B>>,
    to_net:       UnboundedSender<WrappedOverlordMsg<B>>,

    pub from_exec: UnboundedReceiver<ExecResult<S>>,
    to_exec:       UnboundedSender<ExecRequest<S>>,

    pub from_fetch: UnboundedReceiver<OverlordResult<FetchedFullBlock>>,
    to_fetch:       UnboundedSender<OverlordResult<FetchedFullBlock>>,

    pub from_timeout: UnboundedReceiver<TimeoutEvent>,
    to_timeout:       UnboundedSender<TimeoutEvent>,
}

impl<A: Adapter<B, S>, B: Blk, S: St> EventAgent<A, B, S> {
    pub fn new(
        address: Address,
        adapter: &Arc<A>,
        time_config: TimeConfig,
        from_net: UnboundedReceiver<(Context, OverlordMsg<B>)>,
        to_net: UnboundedSender<(Context, OverlordMsg<B>)>,
        from_exec: UnboundedReceiver<ExecResult<S>>,
        to_exec: UnboundedSender<ExecRequest<S>>,
    ) -> Self {
        let (to_fetch, from_fetch) = unbounded();
        let (to_timeout, from_timeout) = unbounded();
        EventAgent {
            address,
            adapter: Arc::<A>::clone(adapter),
            fetch_set: HashSet::new(),
            start_time: Instant::now(),
            time_config,
            from_net,
            to_net,
            from_exec,
            to_exec,
            from_fetch,
            to_fetch,
            from_timeout,
            to_timeout,
        }
    }

    pub fn handle_commit(&mut self, time_config: TimeConfig) {
        self.time_config = time_config;
        self.fetch_set.clear();
    }

    pub fn next_height(&mut self) {
        self.start_time = Instant::now();
    }

    pub async fn transmit(&self, to: Address, msg: OverlordMsg<B>) -> OverlordResult<()> {
        info!(
            "[TRANSMIT]\n\t<{}> -> {}\n\t<message> {} \n\n\n\n\n",
            self.address.tiny_hex(),
            to.tiny_hex(),
            msg
        );

        if self.address == to {
            self.to_net
                .unbounded_send((Context::default(), msg))
                .expect("Net Channel is down! It's meaningless to continue running");
            Ok(())
        } else {
            self.adapter
                .transmit(Context::default(), to, msg)
                .await
                .map_err(OverlordError::net_transmit)
        }
    }

    pub async fn broadcast(&self, msg: OverlordMsg<B>) -> OverlordResult<()> {
        info!(
            "[BROADCAST]\n\t<{}> =>|\n\t<message> {} \n\n\n\n\n",
            self.address.tiny_hex(),
            msg
        );

        self.to_net
            .unbounded_send((Context::default(), msg.clone()))
            .expect("Net Channel is down! It's meaningless to continue running");
        self.adapter
            .broadcast(Context::default(), msg)
            .await
            .map_err(OverlordError::local_broadcast)
    }

    pub fn handle_fetch(
        &mut self,
        fetch_result: OverlordResult<FetchedFullBlock>,
    ) -> OverlordResult<FetchedFullBlock> {
        if let Err(error) = fetch_result {
            if let ErrorInfo::FetchFullBlock(hash) = error.info {
                self.fetch_set.remove(&hash);
                return Err(OverlordError::net_fetch(hash));
            }
            unreachable!()
        } else {
            Ok(fetch_result.unwrap())
        }
    }

    pub fn request_full_block(&self, block: B) {
        let block_hash = block
            .get_block_hash()
            .expect("Unreachable! Block hash has been checked before");
        if self.fetch_set.contains(&block_hash) {
            return;
        }

        info!(
            "[FETCH]\n\t<{}> -> full block\n\t<request> block_hash: {}\n\n\n\n\n",
            self.address.tiny_hex(),
            block_hash.tiny_hex()
        );

        let adapter = Arc::<A>::clone(&self.adapter);
        let to_fetch = self.to_fetch.clone();
        let height = block.get_height();

        tokio::spawn(async move {
            let rst = adapter
                .fetch_full_block(Context::default(), block)
                .await
                .map(|full_block| FetchedFullBlock::new(height, block_hash.clone(), full_block))
                .map_err(|_| OverlordError::net_fetch(block_hash));
            to_fetch
                .unbounded_send(rst)
                .expect("Fetch Channel is down! It's meaningless to continue running");
        });
    }

    pub fn save_and_exec_block(&self, request: ExecRequest<S>) {
        info!(
            "[EXEC]\n\t<{}> -> exec\n\t<request> exec_request: {}\n\n\n\n\n",
            self.address.tiny_hex(),
            request
        );

        self.to_exec
            .unbounded_send(request)
            .expect("Exec Channel is down! It's meaningless to continue running");
    }

    pub fn set_step_timeout(&self, stage: Stage) -> bool {
        let opt = self.compute_timeout(&stage);
        if let Some(delay) = opt {
            let timeout_info = TimeoutInfo::new(delay, stage.into(), self.to_timeout.clone());
            self.set_timeout(timeout_info, delay);
            return true;
        }
        false
    }

    pub fn set_height_timeout(&self) {
        let delay = Duration::from_millis(self.time_config.interval * HEIGHT_RATIO / TIME_DIVISOR);
        let timeout_info =
            TimeoutInfo::new(delay, TimeoutEvent::HeightTimeout, self.to_timeout.clone());
        self.set_timeout(timeout_info, delay);
    }

    pub fn set_sync_timeout(&self, request_id: u64) {
        let delay =
            Duration::from_millis(self.time_config.interval * SYNC_TIMEOUT_RATIO / TIME_DIVISOR);
        let timeout_info = TimeoutInfo::new(
            delay,
            TimeoutEvent::SyncTimeout(request_id),
            self.to_timeout.clone(),
        );
        self.set_timeout(timeout_info, delay);
    }

    pub fn set_clear_timeout(&self, address: Address) {
        let delay =
            Duration::from_millis(self.time_config.interval * CLEAR_TIMEOUT_RATIO / TIME_DIVISOR);
        let timeout_info = TimeoutInfo::new(
            delay,
            TimeoutEvent::ClearTimeout(address),
            self.to_timeout.clone(),
        );
        self.set_timeout(timeout_info, delay);
    }

    pub fn set_timeout(&self, timeout_info: TimeoutInfo, delay: Duration) {
        info!(
            "[SET]\n\t<{}> set timeout\n\t<timeout> {},\n\t<delay> {:?}\n\n\n\n",
            self.address.tiny_hex(),
            timeout_info,
            delay,
        );
        tokio::spawn(async move {
            timeout_info.await;
        });
    }

    fn compute_timeout(&self, stage: &Stage) -> Option<Duration> {
        let config = &self.time_config;
        match stage.step {
            Step::Propose => {
                let timeout =
                    Duration::from_millis(config.interval * config.propose_ratio / TIME_DIVISOR);
                Some(apply_power(timeout, stage.round as u32))
            }
            Step::PreVote => {
                let timeout =
                    Duration::from_millis(config.interval * config.pre_vote_ratio / TIME_DIVISOR);
                Some(apply_power(timeout, stage.round as u32))
            }
            Step::PreCommit => {
                let timeout =
                    Duration::from_millis(config.interval * config.pre_commit_ratio / TIME_DIVISOR);
                Some(apply_power(timeout, stage.round as u32))
            }
            Step::Brake => Some(Duration::from_millis(
                config.interval * config.brake_ratio / TIME_DIVISOR,
            )),
            Step::Commit => {
                let cost = Instant::now() - self.start_time;
                Duration::from_millis(config.interval).checked_sub(cost)
            }
        }
    }
}

fn apply_power(timeout: Duration, power: u32) -> Duration {
    let mut timeout = timeout;
    let mut power = power;
    if power > POWER_CAP {
        power = POWER_CAP;
    }
    timeout *= 2u32.pow(power);
    timeout
}
