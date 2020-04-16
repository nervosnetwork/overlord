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
use futures::{select, StreamExt};
use futures::{FutureExt, SinkExt};
use futures_timer::Delay;
use log::{error, warn};

use crate::auth::{AuthCell, AuthFixedConfig, AuthManage};
use crate::cabinet::Cabinet;
use crate::exec::ExecRequest;
use crate::state::{ProposePrepare, Stage, StateError, StateInfo};
use crate::timeout::TimeoutEvent;
use crate::types::{
    ChokeQC, FetchedFullBlock, PreCommitQC, Proposal, SignedChoke, SignedPreCommit, SignedPreVote,
    SignedProposal, UpdateFrom,
};
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
    agent:   EventAgent<B, S>,

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
        from_net: UnboundedReceiver<(Context, OverlordMsg<B>)>,
        from_exec: UnboundedReceiver<ExecResult<S>>,
        to_exec: UnboundedSender<ExecRequest>,
        wal_path: &str,
    ) -> Self {
        let wal = Wal::new(wal_path);
        let rst = StateInfo::<B>::from_wal(&wal);
        let state = if let Err(e) = rst {
            warn!("Load wal failed! Try to recover state by the adapter, which face security risk if majority auth nodes lost their wal file at the same time");
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
            wal,
            state,
            prepare,
            time_start: Instant::now(),
            adapter: Arc::<A>::clone(adapter),
            cabinet: Cabinet::default(),
            auth: AuthManage::new(auth_fixed_config, current_auth, last_auth),
            agent: EventAgent::new(from_net, from_exec, to_exec),
            phantom_s: PhantomData,
        }
    }

    pub async fn run(mut self) {
        loop {
            select! {
                opt = self.agent.from_net.next() => {
                    if let Err(e) = self.handle_msg(opt).await {
                        // self.adapter.handle_error()
                        error!("{}", e);
                    }
                }
                opt = self.agent.from_exec.next() => {
                    if let Err(e) = self.handle_exec_result(opt.expect("Exec Channel is down!")).await {
                        // self.adapter.handle_error()
                        error!("{}", e);
                    }
                }
                opt = self.agent.from_fetch.next() => {
                    if let Err(e) = self.handle_fetch(opt.expect("Fetch Channel is down!")).await {
                        // self.adapter.handle_error()
                        error!("{}", e);
                    }
                }
                opt = self.agent.from_timeout.next() => {
                    if let Err(e) = self.handle_timeout(opt.expect("Timeout Channel is down!")).await {
                        // self.adapter.handle_error()
                        error!("{}", e);
                    }
                }
            }
        }
    }

    async fn handle_msg(&mut self, opt: Option<WrappedOverlordMsg<B>>) -> Result<(), SMRError> {
        if opt.is_none() {
            return Err(SMRError::ChannelClosed("network ".to_owned()));
        }
        let (context, msg) = opt.unwrap();

        match msg {
            OverlordMsg::SignedProposal(signed_proposal) => {
                self.handle_signed_proposal(signed_proposal).await?;
            }
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

    async fn handle_exec_result(&mut self, exec_result: ExecResult<S>) -> Result<(), SMRError> {
        Ok(())
    }

    async fn handle_fetch(
        &mut self,
        _fetched_full_block: FetchedFullBlock,
    ) -> Result<(), SMRError> {
        Ok(())
    }

    async fn handle_timeout(&mut self, timeout_event: TimeoutEvent) -> Result<(), SMRError> {
        match timeout_event {
            TimeoutEvent::ProposeTimeout(stage) => {}
            TimeoutEvent::PreVoteTimeout(stage) => {}
            TimeoutEvent::PreCommitTimeout(stage) => {}
            TimeoutEvent::BrakeTimeout(stage) => {}
            TimeoutEvent::HeightTimeout(height) => {}
        }
        Ok(())
    }

    async fn handle_signed_proposal(
        &mut self,
        signed_proposal: SignedProposal<B>,
    ) -> Result<(), SMRError> {
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

pub struct EventAgent<B: Blk, S: St> {
    from_net: UnboundedReceiver<WrappedOverlordMsg<B>>,

    from_exec: UnboundedReceiver<ExecResult<S>>,
    to_exec:   UnboundedSender<ExecRequest>,

    from_fetch: UnboundedReceiver<FetchedFullBlock>,
    to_fetch:   UnboundedSender<FetchedFullBlock>,

    from_timeout: UnboundedReceiver<TimeoutEvent>,
    to_timeout:   UnboundedSender<TimeoutEvent>,
}

impl<B: Blk, S: St> EventAgent<B, S> {
    fn new(
        from_net: UnboundedReceiver<(Context, OverlordMsg<B>)>,
        from_exec: UnboundedReceiver<ExecResult<S>>,
        to_exec: UnboundedSender<ExecRequest>,
    ) -> Self {
        let (to_fetch, from_fetch) = unbounded();
        let (to_timeout, from_timeout) = unbounded();
        EventAgent {
            from_net,
            from_exec,
            to_exec,
            from_fetch,
            to_fetch,
            from_timeout,
            to_timeout,
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
    #[display(fmt = "{} channel closed", _0)]
    ChannelClosed(String),
    #[display(fmt = "Other error: {}", _0)]
    Other(String),
}

impl Error for SMRError {}
