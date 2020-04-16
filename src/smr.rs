#![allow(unused_imports)]
#![allow(unused_variables)]

use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;
use std::task::{Context as TaskCx, Poll};
use std::time::{Duration, Instant};
use std::{future::Future, pin::Pin};

use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::stream::{Stream, StreamExt};
use futures::{FutureExt, SinkExt};
use futures_timer::Delay;
use log::{error, warn};

use crate::auth::{AuthCell, AuthFixedConfig, AuthManage};
use crate::cabinet::Cabinet;
use crate::state::{ProposePrepare, Stage, StateError, StateInfo};
use crate::timeout::TimeoutEvent;
use crate::types::{FetchedFullBlock, Proposal, UpdateFrom};
use crate::{
    Adapter, Address, Blk, CommonHex, ExecResult, Height, HeightRange, OverlordConfig, OverlordMsg,
    PriKeyHex, Proof, Round, St, TimeConfig, Wal,
};

const MULTIPLIER_CAP: u32 = 5;

pub type WrappedOverlordMsg<B> = (Context, OverlordMsg<B>);

/// State Machine Replica
pub struct SMR<A: Adapter<B, S>, B: Blk, S: St> {
    state: StateInfo<B>,
    prepare: ProposePrepare<S>,
    /// start time of current height
    time_start: Instant,

    adapter: Arc<A>,
    wal:     Wal,
    cabinet: Cabinet<B>,
    auth:    AuthManage<A, B, S>,

    from_net:     UnboundedReceiver<WrappedOverlordMsg<B>>,
    from_fetch:   UnboundedReceiver<FetchedFullBlock>,
    to_fetch:     UnboundedSender<FetchedFullBlock>,
    from_timeout: UnboundedReceiver<TimeoutEvent>,
    to_timeout:   UnboundedSender<TimeoutEvent>,

    phantom_s: PhantomData<S>,
}

impl<A, B, S> SMR<A, B, S>
where
    A: Adapter<B, S>,
    B: Blk,
    S: St,
{
    pub async fn new(
        auth_fixed_config: AuthFixedConfig,
        adapter: &Arc<A>,
        net_receiver: UnboundedReceiver<(Context, OverlordMsg<B>)>,
        wal_path: &str,
    ) -> Self {
        let (to_fetch, from_fetch) = unbounded();
        let (to_timeout, from_timeout) = unbounded();

        let wal = Wal::new(wal_path);
        let rst = StateInfo::<B>::from_wal(&wal);
        let state = if let Err(e) = rst {
            error!("Load wal failed! Try to recover state by the adapter, which face security risk if majority auth nodes lost their wal file at the same time");
            recover_state_by_adapter(adapter).await
        } else {
            rst.unwrap()
        };

        let height = state.stage.height;

        let (prepare, current_config) = recover_propose_prepare_and_config(adapter, height).await;
        let last_config = if height > 0 {
            Some(
                get_exec_result(adapter, state.stage.height - 1)
                    .await
                    .unwrap()
                    .unwrap()
                    .consensus_config,
            )
        } else {
            None
        };

        let current_auth = AuthCell::new(current_config.auth_config, &auth_fixed_config.address);
        let last_auth: Option<AuthCell> =
            last_config.map(|config| AuthCell::new(config.auth_config, &auth_fixed_config.address));

        SMR {
            state,
            prepare,
            time_start: Instant::now(),
            adapter: Arc::<A>::clone(adapter),
            wal: Wal::new(wal_path),
            cabinet: Cabinet::default(),
            auth: AuthManage::new(auth_fixed_config, current_auth, last_auth),
            from_net: net_receiver,
            from_fetch,
            to_fetch,
            from_timeout,
            to_timeout,
            phantom_s: PhantomData,
        }
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            while let Some(err) = self.next().await {
                // Todo: Add handle error here
                error!("smr error {:?}", err);
            }
        });
    }

    fn process_msg(&mut self, wrapped_msg: WrappedOverlordMsg<B>) -> Result<(), SMRError> {
        let (context, msg) = wrapped_msg;

        match msg {
            OverlordMsg::SignedProposal(signed_proposal) => {}
            OverlordMsg::SignedPreVote(signed_pre_vote) => {}
            OverlordMsg::SignedPreCommit(signed_pre_commit) => {}
            OverlordMsg::SignedChoke(signed_choke) => {}
            OverlordMsg::PreVoteQC(pre_vote_qc) => {}
            OverlordMsg::PreCommitQC(pre_commit_qc) => {}
            _ => {
                // ToDo: synchronization
            }
        }

        Ok(())
    }

    fn process_fetch(&mut self, _fetched_full_block: FetchedFullBlock) -> Result<(), SMRError> {
        Ok(())
    }

    fn process_timeout(&mut self, timeout_event: TimeoutEvent) -> Result<(), SMRError> {
        match timeout_event {
            TimeoutEvent::ProposeTimeout(stage) => {}
            TimeoutEvent::PreVoteTimeout(stage) => {}
            TimeoutEvent::PreCommitTimeout(stage) => {}
            TimeoutEvent::BrakeTimeout(stage) => {}
            TimeoutEvent::HeightTimeout(height) => {}
        }
        Ok(())
    }
}

async fn recover_state_by_adapter<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
) -> StateInfo<B> {
    let height = adapter.get_latest_height(Context::default()).await.expect(
        "Cannot get the latest height from the adapter! It's meaningless to continue running",
    );
    StateInfo::new(height)
}

async fn recover_propose_prepare_and_config<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
    latest_height: Height,
) -> (ProposePrepare<S>, OverlordConfig) {
    let (block, proof) = get_block_with_proof(adapter, latest_height)
        .await
        .unwrap()
        .unwrap();
    let hash = block.get_block_hash();
    let exec_height = block.get_exec_height();
    let mut time_config = TimeConfig::default();
    let mut block_states = vec![];
    let mut latest_config = OverlordConfig::default();
    for h in exec_height + 1..=latest_height {
        let exec_result = get_exec_result(adapter, h).await.unwrap().unwrap();
        block_states.push(exec_result.block_states);
        time_config = exec_result.consensus_config.time_config.clone();
        latest_config = exec_result.consensus_config.clone();
    }
    (
        ProposePrepare::new(time_config, latest_height, block_states, proof, hash),
        latest_config,
    )
}

async fn get_block_with_proof<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
    height: Height,
) -> Result<Option<(B, Proof)>, SMRError> {
    let vec = adapter
        .get_block_with_proofs(Context::default(), HeightRange::new(height, 1))
        .await
        .map_err(SMRError::GetBlock)?;
    if vec.is_empty() {
        return Ok(None);
    }
    Ok(Some(vec[0].clone()))
}

async fn get_exec_result<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
    height: Height,
) -> Result<Option<ExecResult<S>>, SMRError> {
    let opt = get_block_with_proof(adapter, height).await?;
    if let Some((block, proof)) = opt {
        let full_block = adapter
            .fetch_full_block(Context::default(), &block)
            .await
            .map_err(SMRError::FetchFullBlock)?;
        let rst = adapter
            .save_and_exec_block_with_proof(Context::default(), height, full_block, proof.clone())
            .await
            .map_err(SMRError::ExecBlock)?;
        Ok(Some(rst))
    } else {
        Ok(None)
    }
}

impl<A, B, S> Stream for SMR<A, B, S>
where
    A: Adapter<B, S>,
    B: Blk,
    S: St,
{
    type Item = SMRError;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskCx) -> Poll<Option<Self::Item>> {
        let mut net_ready = true;
        let mut fetch_ready = true;
        let mut timeout_ready = true;

        loop {
            match self.from_net.poll_next_unpin(cx) {
                Poll::Pending => net_ready = false,

                Poll::Ready(opt) => {
                    let wrapped_msg =
                        opt.expect("Network is down! It's meaningless to continue running");
                    if let Err(e) = self.process_msg(wrapped_msg) {
                        return Poll::Ready(Some(e));
                    }
                }
            };

            match self.from_fetch.poll_next_unpin(cx) {
                Poll::Pending => fetch_ready = false,

                Poll::Ready(opt) => {
                    let fetched_full_block =
                        opt.expect("Fetch Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.process_fetch(fetched_full_block) {
                        return Poll::Ready(Some(e));
                    }
                }
            }

            match self.from_timeout.poll_next_unpin(cx) {
                Poll::Pending => timeout_ready = false,

                Poll::Ready(opt) => {
                    let timeout_event =
                        opt.expect("Timeout Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.process_timeout(timeout_event) {
                        return Poll::Ready(Some(e));
                    }
                }
            }

            if !net_ready && !fetch_ready && !timeout_ready {
                return Poll::Pending;
            }
        }
    }
}

#[derive(Debug, Display)]
pub enum SMRError {
    #[display(fmt = "{}", _0)]
    State(StateError),
    #[display(fmt = "get block failed: {}", _0)]
    GetBlock(Box<dyn Error + Send>),
    #[display(fmt = "fetch full block failed: {}", _0)]
    FetchFullBlock(Box<dyn Error + Send>),
    #[display(fmt = "exec block failed: {}", _0)]
    ExecBlock(Box<dyn Error + Send>),
    #[display(fmt = "Other error: {}", _0)]
    Other(String),
}

impl Error for SMRError {}
