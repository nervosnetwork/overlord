use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::StreamExt;
use log::info;
use tokio::sync::RwLock;

use crate::state::{Stage, Step};
use crate::types::FetchedFullBlock;
use crate::utils::exec::ExecRequest;
use crate::utils::sync::{CLEAR_TIMEOUT_RATIO, HEIGHT_RATIO, SYNC_TIMEOUT_RATIO};
use crate::utils::timeout::{TimeoutEvent, TimeoutInfo};
use crate::{
    Adapter, Address, Blk, ExecResult, FullBlk, Hash, OverlordError, OverlordMsg, OverlordResult,
    St, TimeConfig, TinyHex,
};

const POWER_CAP: u32 = 5;
const TIME_DIVISOR: u64 = 10;

#[derive(Debug, Display)]
pub enum ChannelMsg<B: Blk, F: FullBlk<B>, S: St> {
    #[display(fmt = "PreHandleMsg: {{ context: {:?}, msg: {} }}", _0, _1)]
    PreHandleMsg(Context, OverlordMsg<B, F>),
    #[display(fmt = "HandleMsg: {{ context: {:?}, msg: {} }}", _0, _1)]
    HandleMsg(Context, OverlordMsg<B, F>),
    #[display(
        fmt = "HandleMsgError: {{ context: {:?}, msg: {}, error: {} }}",
        _0,
        _1,
        _2
    )]
    HandleMsgError(Context, OverlordMsg<B, F>, OverlordError),
    #[display(
        fmt = "HandleTimeoutError: {{ context: {:?}, timeout: {}, error: {} }}",
        _0,
        _1,
        _2
    )]
    HandleTimeoutError(Context, TimeoutEvent, OverlordError),
    #[display(fmt = "ExecRequest: {{ context: {:?}, exec_request: {} }}", _0, _1)]
    ExecRequest(Context, ExecRequest<B, F, S>),
    #[display(fmt = "ExecResult: {{ context: {:?}, exec_result: {} }}", _0, _1)]
    ExecResult(Context, ExecResult<S>),
    #[display(fmt = "TimeoutEvent: {{ context: {:?}, timeout_event: {} }}", _0, _1)]
    TimeoutEvent(Context, TimeoutEvent),
}

pub struct EventAgent<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St> {
    address:     Address,
    adapter:     Arc<A>,
    time_config: Arc<RwLock<TimeConfig>>,
    start_time:  Arc<RwLock<Instant>>, // start time of current height
    fetch_set:   Arc<RwLock<HashSet<Hash>>>,

    smr_receiver: Arc<RwLock<UnboundedReceiver<ChannelMsg<B, F, S>>>>,
    smr_sender:   UnboundedSender<ChannelMsg<B, F, S>>,
    exec_sender:  UnboundedSender<ChannelMsg<B, F, S>>,
}

impl<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St> EventAgent<A, B, F, S> {
    pub fn new(
        address: Address,
        adapter: &Arc<A>,
        time_config: TimeConfig,
        smr_receiver: UnboundedReceiver<ChannelMsg<B, F, S>>,
        smr_sender: UnboundedSender<ChannelMsg<B, F, S>>,
        exec_sender: UnboundedSender<ChannelMsg<B, F, S>>,
    ) -> Self {
        EventAgent {
            address,
            adapter: Arc::<A>::clone(adapter),
            fetch_set: Arc::new(RwLock::new(HashSet::new())),
            start_time: Arc::new(RwLock::new(Instant::now())),
            time_config: Arc::new(RwLock::new(time_config)),
            smr_receiver: Arc::new(RwLock::new(smr_receiver)),
            smr_sender,
            exec_sender,
        }
    }

    pub async fn next_channel_msg(&self) -> ChannelMsg<B, F, S> {
        self.smr_receiver
            .write()
            .await
            .next()
            .await
            .expect("SMR Channel is down! It's meaningless to continue running")
    }

    pub async fn handle_commit(&self, time_config: TimeConfig) {
        *self.time_config.write().await = time_config;
    }

    pub async fn next_height(&self) {
        self.fetch_set.write().await.clear();
        *self.start_time.write().await = Instant::now();
    }

    pub async fn remove_block_hash(&self, block_hash: Hash) {
        self.fetch_set.write().await.remove(&block_hash);
    }

    pub async fn transmit(
        &self,
        ctx: Context,
        to: Address,
        msg: OverlordMsg<B, F>,
    ) -> OverlordResult<()> {
        if self.address == to {
            self.send_to_myself(ChannelMsg::PreHandleMsg(ctx, msg));
            Ok(())
        } else {
            info!(
                "[TRANSMIT]\n\t<{}> -> {}\n\t<message> {} \n\n\n\n\n",
                self.address.tiny_hex(),
                to.tiny_hex(),
                msg
            );
            self.adapter
                .transmit(ctx, to, msg)
                .await
                .map_err(OverlordError::net_transmit)
        }
    }

    pub async fn broadcast(&self, ctx: Context, msg: OverlordMsg<B, F>) -> OverlordResult<()> {
        info!(
            "[BROADCAST]\n\t<{}> =>|\n\t<message> {} \n\n\n\n\n",
            self.address.tiny_hex(),
            msg
        );

        self.send_to_myself(ChannelMsg::PreHandleMsg(ctx.clone(), msg.clone()));
        self.adapter
            .broadcast(ctx, msg)
            .await
            .map_err(OverlordError::local_broadcast)
    }

    pub fn send_to_myself(&self, msg: ChannelMsg<B, F, S>) {
        info!(
            "[TRANSMIT]\n\t<{}> -> myself\n\t<message> {} \n\n\n\n\n",
            self.address.tiny_hex(),
            msg
        );
        self.smr_sender
            .unbounded_send(msg)
            .expect("Net Channel is down! It's meaningless to continue running");
    }

    pub fn replay_msg(&self, ctx: Context, msg: OverlordMsg<B, F>) {
        info!(
            "[Replay]\n\t<{}>\n\t<message> {} \n\n\n\n\n",
            self.address.tiny_hex(),
            msg
        );
        self.smr_sender
            .unbounded_send(ChannelMsg::PreHandleMsg(ctx, msg))
            .expect("Net Channel is down! It's meaningless to continue running");
    }

    pub async fn request_full_block(&self, ctx: Context, block: B) {
        let block_hash = block
            .get_block_hash()
            .expect("Unreachable! Block hash has been checked before");
        if self.fetch_set.read().await.contains(&block_hash) {
            return;
        }

        info!(
            "[FETCH]\n\t<{}> -> full block\n\t<request> block_hash: {}\n\n\n\n\n",
            self.address.tiny_hex(),
            block_hash.tiny_hex()
        );

        let adapter = Arc::<A>::clone(&self.adapter);
        let smr_sender = self.smr_sender.clone();
        let height = block.get_height();
        tokio::spawn(async move {
            let rst = adapter
                .fetch_full_block(ctx.clone(), block)
                .await
                .map(|full_block| FetchedFullBlock::new(height, block_hash.clone(), full_block))
                .map_err(|e| OverlordError::net_fetch(block_hash.clone(), e));
            if let Err(e) = rst {
                smr_sender
                    .unbounded_send(ChannelMsg::HandleMsgError(
                        ctx,
                        FetchedFullBlock::new(height, block_hash, F::default()).into(),
                        e,
                    ))
                    .expect("Net Channel is down! It's meaningless to continue running");
            } else {
                smr_sender
                    .unbounded_send(ChannelMsg::PreHandleMsg(ctx, rst.unwrap().into()))
                    .expect("Net Channel is down! It's meaningless to continue running");
            }
        });
    }

    pub fn exec_block(&self, ctx: Context, request: ExecRequest<B, F, S>) {
        info!(
            "[EXEC]\n\t<{}> -> exec\n\t<request> exec_request: {}\n\n\n\n\n",
            self.address.tiny_hex(),
            request
        );

        self.exec_sender
            .unbounded_send(ChannelMsg::ExecRequest(ctx, request))
            .expect("Exec Channel is down! It's meaningless to continue running");
    }

    pub async fn set_step_timeout(&self, ctx: Context, stage: Stage) -> bool {
        let opt = self.compute_timeout(&stage).await;
        if let Some(delay) = opt {
            let timeout_info = TimeoutInfo::new(ctx, delay, stage.into(), self.smr_sender.clone());
            self.set_timeout(timeout_info, delay);
            return true;
        }
        false
    }

    pub async fn set_height_timeout(&self, ctx: Context) {
        let delay = Duration::from_millis(
            self.time_config.read().await.interval * HEIGHT_RATIO / TIME_DIVISOR,
        );
        let timeout_info = TimeoutInfo::new(
            ctx,
            delay,
            TimeoutEvent::HeightTimeout,
            self.smr_sender.clone(),
        );
        self.set_timeout(timeout_info, delay);
    }

    pub async fn set_sync_timeout(&self, ctx: Context, request_id: u64) {
        let delay = Duration::from_millis(
            self.time_config.read().await.interval * SYNC_TIMEOUT_RATIO / TIME_DIVISOR,
        );
        let timeout_info = TimeoutInfo::new(
            ctx,
            delay,
            TimeoutEvent::SyncTimeout(request_id),
            self.smr_sender.clone(),
        );
        self.set_timeout(timeout_info, delay);
    }

    pub async fn set_clear_timeout(&self, ctx: Context, address: Address) {
        let delay = Duration::from_millis(
            self.time_config.read().await.interval * CLEAR_TIMEOUT_RATIO / TIME_DIVISOR,
        );
        let timeout_info = TimeoutInfo::new(
            ctx,
            delay,
            TimeoutEvent::ClearTimeout(address),
            self.smr_sender.clone(),
        );
        self.set_timeout(timeout_info, delay);
    }

    pub fn set_timeout(&self, timeout_info: TimeoutInfo<B, F, S>, delay: Duration) {
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

    async fn compute_timeout(&self, stage: &Stage) -> Option<Duration> {
        let config = self.time_config.read().await;
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
                let cost = Instant::now() - *self.start_time.read().await;
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
