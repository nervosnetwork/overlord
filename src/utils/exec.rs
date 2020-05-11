use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;

use crate::utils::agent::{WrappedExecRequest, WrappedExecResult};
use crate::{Adapter, Blk, FullBlk, Height, St, INIT_HEIGHT};

#[derive(Display)]
#[display(
    fmt = "{{ height: {}, last_exec_resp: {}, last_commit_exec_resp: {} }}",
    height,
    last_exec_resp,
    last_commit_exec_resp
)]
pub struct ExecRequest<B: Blk, F: FullBlk<B>, S: St> {
    height:                Height,
    full_block:            F,
    last_exec_resp:        S,
    last_commit_exec_resp: S,
    phantom:               PhantomData<B>,
}

impl<B: Blk, F: FullBlk<B>, S: St> ExecRequest<B, F, S> {
    pub fn new(height: Height, full_block: F, last_exec_resp: S, last_commit_exec_resp: S) -> Self {
        ExecRequest {
            height,
            full_block,
            last_exec_resp,
            last_commit_exec_resp,
            phantom: PhantomData,
        }
    }
}

pub struct Exec<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St> {
    adapter:  Arc<A>,
    from_smr: UnboundedReceiver<WrappedExecRequest<B, F, S>>,
    to_smr:   UnboundedSender<WrappedExecResult<S>>,

    last_exec_resp:   Option<S>,
    last_exec_height: Height,

    phantom: PhantomData<B>,
}

impl<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St> Exec<A, B, F, S> {
    pub fn new(
        adapter: &Arc<A>,
        from_smr: UnboundedReceiver<WrappedExecRequest<B, F, S>>,
        to_smr: UnboundedSender<WrappedExecResult<S>>,
    ) -> Self {
        Exec {
            adapter: Arc::<A>::clone(adapter),
            from_smr,
            to_smr,
            last_exec_resp: None,
            last_exec_height: INIT_HEIGHT,
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

    async fn save_and_exec_block(&mut self, ctx: Context, request: ExecRequest<B, F, S>) {
        let height = request.height;
        // sync block will make exec's last_exec_height < height - 1
        if self.last_exec_resp.is_none() || self.last_exec_height < height - 1 {
            self.last_exec_resp = Some(request.last_exec_resp.clone());
        }

        let exec_result = self
            .adapter
            .exec_full_block(
                ctx.clone(),
                height,
                request.full_block,
                self.last_exec_resp.clone().unwrap(),
                request.last_commit_exec_resp,
            )
            .await
            .expect("Execution is down! It's meaningless to continue running");

        self.last_exec_resp = Some(exec_result.block_states.state.clone());
        self.last_exec_height = height;

        self.to_smr
            .unbounded_send((ctx, exec_result))
            .expect("Exec Channel is down! It's meaningless to continue running");
    }
}
