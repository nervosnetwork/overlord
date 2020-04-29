use std::marker::PhantomData;
use std::sync::Arc;

use bytes::Bytes;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;

use crate::utils::agent::{WrappedExecRequest, WrappedExecResult};
use crate::{Adapter, Blk, Height, St};

#[derive(Display)]
#[display(
    fmt = "{{ height: {}, last_exec_resp: {}, last_commit_exec_resp: {} }}",
    height,
    last_exec_resp,
    last_commit_exec_resp
)]
pub struct ExecRequest<S: St> {
    height:                Height,
    full_block:            Bytes,
    last_exec_resp:        S,
    last_commit_exec_resp: S,
}

impl<S: St> ExecRequest<S> {
    pub fn new(
        height: Height,
        full_block: Bytes,
        last_exec_resp: S,
        last_commit_exec_resp: S,
    ) -> Self {
        ExecRequest {
            height,
            full_block,
            last_exec_resp,
            last_commit_exec_resp,
        }
    }
}

pub struct Exec<A: Adapter<B, S>, B: Blk, S: St> {
    adapter:  Arc<A>,
    from_smr: UnboundedReceiver<WrappedExecRequest<S>>,
    to_smr:   UnboundedSender<WrappedExecResult<S>>,

    phantom: PhantomData<B>,
}

impl<A: Adapter<B, S>, B: Blk, S: St> Exec<A, B, S> {
    pub fn new(
        adapter: &Arc<A>,
        from_smr: UnboundedReceiver<WrappedExecRequest<S>>,
        to_smr: UnboundedSender<WrappedExecResult<S>>,
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
            loop {
                let (ctx, request) = self
                    .from_smr
                    .next()
                    .await
                    .expect("SMR is down! It's meaningless to continue running");
                self.save_and_exec_block(ctx, request).await;
            }
        });
    }

    async fn save_and_exec_block(&self, ctx: Context, request: ExecRequest<S>) {
        let exec_result = self
            .adapter
            .exec_full_block(
                ctx.clone(),
                request.height,
                request.full_block,
                request.last_exec_resp,
                request.last_commit_exec_resp,
                false,
            )
            .await
            .expect("Execution is down! It's meaningless to continue running");
        self.to_smr
            .unbounded_send((ctx, exec_result))
            .expect("Exec Channel is down! It's meaningless to continue running");
    }
}
