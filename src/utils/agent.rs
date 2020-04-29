use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use log::info;

use crate::error::ErrorInfo;
use crate::state::{Stage, Step};
use crate::types::FetchedFullBlock;
use crate::utils::exec::ExecRequest;
use crate::utils::sync::{CLEAR_TIMEOUT_RATIO, HEIGHT_RATIO, SYNC_TIMEOUT_RATIO};
use crate::utils::timeout::{TimeoutEvent, TimeoutInfo};
use crate::{
    Adapter, Address, Blk, ExecResult, Hash, OverlordError, OverlordMsg, OverlordResult, St,
    TimeConfig, TinyHex,
};

const POWER_CAP: u32 = 5;
const TIME_DIVISOR: u64 = 10;

pub type WrappedOverlordMsg<B> = (Context, OverlordMsg<B>);
pub type WrappedExecRequest<S> = (Context, ExecRequest<S>);
pub type WrappedExecResult<S> = (Context, ExecResult<S>);
pub type WrappedFetchedFullBlock = (Context, OverlordResult<FetchedFullBlock>);
pub type WrappedTimeoutEvent = (Context, TimeoutEvent);

pub struct EventAgent<A: Adapter<B, S>, B: Blk, S: St> {
    address:     Address,
    adapter:     Arc<A>,
    time_config: TimeConfig,
    start_time:  Instant, // start time of current height
    fetch_set:   HashSet<Hash>,

    pub from_net: UnboundedReceiver<WrappedOverlordMsg<B>>,
    to_net:       UnboundedSender<WrappedOverlordMsg<B>>,

    pub from_exec: UnboundedReceiver<WrappedExecResult<S>>,
    to_exec:       UnboundedSender<WrappedExecRequest<S>>,

    pub from_fetch: UnboundedReceiver<WrappedFetchedFullBlock>,
    to_fetch:       UnboundedSender<WrappedFetchedFullBlock>,

    pub from_timeout: UnboundedReceiver<WrappedTimeoutEvent>,
    to_timeout:       UnboundedSender<WrappedTimeoutEvent>,
}

impl<A: Adapter<B, S>, B: Blk, S: St> EventAgent<A, B, S> {
    pub fn new(
        address: Address,
        adapter: &Arc<A>,
        time_config: TimeConfig,
        from_net: UnboundedReceiver<WrappedOverlordMsg<B>>,
        to_net: UnboundedSender<WrappedOverlordMsg<B>>,
        from_exec: UnboundedReceiver<WrappedExecResult<S>>,
        to_exec: UnboundedSender<WrappedExecRequest<S>>,
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

    pub async fn transmit(
        &self,
        ctx: Context,
        to: Address,
        msg: OverlordMsg<B>,
    ) -> OverlordResult<()> {
        info!(
            "[TRANSMIT]\n\t<{}> -> {}\n\t<message> {} \n\n\n\n\n",
            self.address.tiny_hex(),
            to.tiny_hex(),
            msg
        );

        if self.address == to {
            self.send_to_myself(ctx, msg);
            Ok(())
        } else {
            self.adapter
                .transmit(ctx, to, msg)
                .await
                .map_err(OverlordError::net_transmit)
        }
    }

    pub async fn broadcast(&self, ctx: Context, msg: OverlordMsg<B>) -> OverlordResult<()> {
        info!(
            "[BROADCAST]\n\t<{}> =>|\n\t<message> {} \n\n\n\n\n",
            self.address.tiny_hex(),
            msg
        );

        self.send_to_myself(ctx.clone(), msg.clone());
        self.adapter
            .broadcast(ctx, msg)
            .await
            .map_err(OverlordError::local_broadcast)
    }

    pub fn send_to_myself(&self, ctx: Context, msg: OverlordMsg<B>) {
        self.to_net
            .unbounded_send((ctx, msg))
            .expect("Net Channel is down! It's meaningless to continue running");
    }

    pub fn handle_fetch(
        &mut self,
        fetch_result: OverlordResult<FetchedFullBlock>,
    ) -> OverlordResult<FetchedFullBlock> {
        if let Err(error) = fetch_result {
            if let ErrorInfo::FetchFullBlock(hash, e) = error.info {
                self.fetch_set.remove(&hash);
                return Err(OverlordError::net_fetch(hash, e));
            }
            unreachable!()
        } else {
            Ok(fetch_result.unwrap())
        }
    }

    pub fn request_full_block(&self, ctx: Context, block: B) {
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
                .fetch_full_block(ctx.clone(), block)
                .await
                .map(|full_block| FetchedFullBlock::new(height, block_hash.clone(), full_block))
                .map_err(|e| OverlordError::net_fetch(block_hash, e));
            to_fetch
                .unbounded_send((ctx, rst))
                .expect("Fetch Channel is down! It's meaningless to continue running");
        });
    }

    pub fn exec_block(&self, ctx: Context, request: ExecRequest<S>) {
        info!(
            "[EXEC]\n\t<{}> -> exec\n\t<request> exec_request: {}\n\n\n\n\n",
            self.address.tiny_hex(),
            request
        );

        self.to_exec
            .unbounded_send((ctx, request))
            .expect("Exec Channel is down! It's meaningless to continue running");
    }

    pub fn set_step_timeout(&self, ctx: Context, stage: Stage) -> bool {
        let opt = self.compute_timeout(&stage);
        if let Some(delay) = opt {
            let timeout_info = TimeoutInfo::new(ctx, delay, stage.into(), self.to_timeout.clone());
            self.set_timeout(timeout_info, delay);
            return true;
        }
        false
    }

    pub fn set_height_timeout(&self, ctx: Context) {
        let delay = Duration::from_millis(self.time_config.interval * HEIGHT_RATIO / TIME_DIVISOR);
        let timeout_info = TimeoutInfo::new(
            ctx,
            delay,
            TimeoutEvent::HeightTimeout,
            self.to_timeout.clone(),
        );
        self.set_timeout(timeout_info, delay);
    }

    pub fn set_sync_timeout(&self, ctx: Context, request_id: u64) {
        let delay =
            Duration::from_millis(self.time_config.interval * SYNC_TIMEOUT_RATIO / TIME_DIVISOR);
        let timeout_info = TimeoutInfo::new(
            ctx,
            delay,
            TimeoutEvent::SyncTimeout(request_id),
            self.to_timeout.clone(),
        );
        self.set_timeout(timeout_info, delay);
    }

    pub fn set_clear_timeout(&self, ctx: Context, address: Address) {
        let delay =
            Duration::from_millis(self.time_config.interval * CLEAR_TIMEOUT_RATIO / TIME_DIVISOR);
        let timeout_info = TimeoutInfo::new(
            ctx,
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
