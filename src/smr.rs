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
use log::error;

use crate::auth::{AuthFixedConfig, AuthManage};
use crate::cabinet::Cabinet;
use crate::state::{Stage, StateError, StateInfo};
use crate::timeout::TimeoutEvent;
use crate::types::{FetchedFullBlock, Proposal, UpdateFrom};
use crate::{
    Adapter, Address, Blk, CommonHex, HeightRange, OverlordMsg, PriKeyHex, Round, St, TimeConfig,
    Wal,
};

const MULTIPLIER_CAP: u32 = 5;

pub type WrappedOverlordMsg<B> = (Context, OverlordMsg<B>);

pub struct StateMachine<A: Adapter<B, S>, B: Blk, S: St> {
    state:        StateInfo<B>,
    height_start: Instant,

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

impl<A, B, S> StateMachine<A, B, S>
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
        // check wal
        let wal = Wal::new(wal_path);

        let v = StateInfo::<B>::from_wal(&wal);
        let state = if let Err(e) = v {
            error!("Load wal failed! Try to recover state from the adapter, which face a little security risk for auth nodes");
            get_state_from_adapter(adapter).await
        } else {
            v.unwrap()
        };

        StateMachine {
            state: StateInfo::default(),
            height_start: Instant::now(),
            adapter: Arc::<A>::clone(adapter),
            wal: Wal::new(wal_path),
            cabinet: Cabinet::default(),
            auth: AuthManage::new(auth_fixed_config),
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

async fn get_state_from_adapter<A: Adapter<B, S>, B: Blk, S: St>(adapter: &Arc<A>) -> StateInfo<B> {
    let expect_str =
        "Nothing can get from both wal and adapter! It's meaningless to continue running";
    let ctx = Context::default();
    let latest_height = adapter
        .get_latest_height(ctx.clone())
        .await
        .expect(expect_str);
    let vec = adapter
        .get_block_with_proofs(ctx.clone(), HeightRange::new(latest_height, 1))
        .await
        .expect(expect_str);
    println!("{}", vec.len());
    let (block, proof) = vec[0].clone();
    let full_block = adapter
        .fetch_full_block(ctx.clone(), &block)
        .await
        .expect(expect_str);
    let exec_result = adapter
        .save_and_exec_block_with_proof(ctx, latest_height, full_block, proof.clone())
        .await
        .expect(expect_str);
    let time_config = exec_result.consensus_config.time_config;
    StateInfo::new(latest_height, time_config, proof)
}

impl<A, B, S> Stream for StateMachine<A, B, S>
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
    #[display(fmt = "Other error: {}", _0)]
    Other(String),
}

impl Error for SMRError {}
