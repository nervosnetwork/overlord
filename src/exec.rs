use std::marker::PhantomData;
use std::sync::Arc;

use bytes::Bytes;
use creep::Context;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;

use crate::{Adapter, Blk, ExecResult, Height, Proof, St};

pub struct ExecRequest {
    height:     Height,
    full_block: Bytes,
    proof:      Proof,
}

impl ExecRequest {
    pub fn new(height: Height, full_block: Bytes, proof: Proof) -> Self {
        ExecRequest {
            height,
            full_block,
            proof,
        }
    }
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

    async fn save_and_exec_block(&self, request: ExecRequest) {
        let exec_result = self
            .adapter
            .save_and_exec_block_with_proof(
                Context::default(),
                request.height,
                request.full_block,
                request.proof,
            )
            .await
            .expect("Execution is down! It's meaningless to continue running");
        self.to_smr
            .unbounded_send(exec_result)
            .expect("Exec Channel is down! It's meaningless to continue running");
    }
}
