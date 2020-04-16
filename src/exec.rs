#![allow(unused_imports)]
#![allow(unused_variables)]

use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;
use std::task::{Context as TaskCx, Poll};
use std::time::{Duration, Instant};
use std::{future::Future, pin::Pin};

use bytes::Bytes;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{unbounded, SendError, UnboundedReceiver, UnboundedSender};
use futures::stream::{Stream, StreamExt};
use futures::{FutureExt, SinkExt, TryFutureExt};
use futures_timer::Delay;
use log::error;

use crate::auth::{AuthFixedConfig, AuthManage};
use crate::cabinet::Cabinet;
use crate::smr::SMRError::ExecBlock;
use crate::state::{Stage, StateError, StateInfo};
use crate::timeout::TimeoutEvent;
use crate::types::{FetchedFullBlock, Proposal, UpdateFrom};
use crate::{
    Adapter, Address, Blk, BlockState, CommonHex, ExecResult, Height, HeightRange, OverlordMsg,
    PriKeyHex, Proof, Round, St, TimeConfig, Wal,
};

pub struct ExecRequest {
    height:     Height,
    full_block: Bytes,
    proof:      Proof,
}

pub struct Exec<A: Adapter<B, S>, B: Blk, S: St> {
    adapter:  Arc<A>,
    from_smr: UnboundedReceiver<ExecRequest>,
    to_smr:   UnboundedSender<ExecResult<S>>,

    phantom: PhantomData<B>,
}

impl<A: Adapter<B, S>, B: Blk, S: St> Exec<A, B, S> {
    pub fn new(
        adapter: &Arc<A>,
        from_smr: UnboundedReceiver<ExecRequest>,
        to_smr: UnboundedSender<ExecResult<S>>,
    ) -> Self {
        Exec {
            adapter: Arc::<A>::clone(adapter),
            from_smr,
            to_smr,
            phantom: PhantomData,
        }
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            while let Some(err) = self.next().await {
                // Todo: Add handle error here
                error!("exec error {:?}", err);
            }
        });
    }

    fn save_and_exec_block(&self, request: ExecRequest) {
        let adapter = Arc::<A>::clone(&self.adapter);
        let to_smr = self.to_smr.clone();
        tokio::spawn(async move {
            let exec_result = adapter
                .save_and_exec_block_with_proof(
                    Context::default(),
                    request.height,
                    request.full_block,
                    request.proof,
                )
                .await
                .expect("Execution is down! It's meaningless to continue running");
            to_smr
                .unbounded_send(exec_result)
                .expect("Exec Channel is down! It's meaningless to continue running");
        });
    }
}

impl<A, B, S> Stream for Exec<A, B, S>
where
    A: Adapter<B, S>,
    B: Blk,
    S: St,
{
    type Item = ExecError;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskCx) -> Poll<Option<Self::Item>> {
        let mut smr_ready = true;

        loop {
            match self.from_smr.poll_next_unpin(cx) {
                Poll::Pending => smr_ready = false,

                Poll::Ready(opt) => {
                    let wrapped_msg =
                        opt.expect("SMR is down! It's meaningless to continue running");

                    self.save_and_exec_block(wrapped_msg);
                }
            };

            if !smr_ready {
                return Poll::Pending;
            }
        }
    }
}

#[derive(Debug, Display)]
pub struct ExecError;

impl Error for ExecError {}
