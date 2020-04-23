use std::marker::PhantomData;
use std::sync::Arc;

use bytes::Bytes;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;

use crate::{Adapter, Blk, ExecResult, Height, Proof, St};

#[derive(Display)]
#[display(fmt = "{{ height: {}, proof: {} }}", height, proof)]
pub struct ExecRequest<S: St> {
    height:                Height,
    full_block:            Bytes,
    proof:                 Proof,
    last_exec_resp:        S,
    last_commit_exec_resp: S,
}

impl<S: St> ExecRequest<S> {
    pub fn new(
        height: Height,
        full_block: Bytes,
        proof: Proof,
        last_exec_resp: S,
        last_commit_exec_resp: S,
    ) -> Self {
        ExecRequest {
            height,
            full_block,
            proof,
            last_exec_resp,
            last_commit_exec_resp,
        }
    }
}

pub struct Exec<A: Adapter<B, S>, B: Blk, S: St> {
    adapter:  Arc<A>,
    from_smr: UnboundedReceiver<ExecRequest<S>>,
    to_smr:   UnboundedSender<ExecResult<S>>,

    phantom: PhantomData<B>,
}

impl<A: Adapter<B, S>, B: Blk, S: St> Exec<A, B, S> {
    pub fn new(
        adapter: &Arc<A>,
        from_smr: UnboundedReceiver<ExecRequest<S>>,
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
            loop {
                let request = self
                    .from_smr
                    .next()
                    .await
                    .expect("SMR is down! It's meaningless to continue running");
                self.save_and_exec_block(request).await;
            }
        });
    }

    async fn save_and_exec_block(&self, request: ExecRequest<S>) {
        let exec_result = self
            .adapter
            .save_and_exec_block_with_proof(
                Context::default(),
                request.height,
                request.full_block,
                request.proof,
                request.last_exec_resp,
                request.last_commit_exec_resp,
            )
            .await
            .expect("Execution is down! It's meaningless to continue running");
        self.to_smr
            .unbounded_send(exec_result)
            .expect("Exec Channel is down! It's meaningless to continue running");
    }
}
