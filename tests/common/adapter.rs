use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::UnboundedSender;
use overlord::{
    Adapter, Address, BlockState, ChannelMsg, DefaultCrypto, ExecResult, FullBlk, Hash, Height,
    HeightRange, OverlordError, OverlordMsg, Proof, TinyHex,
};

use crate::common::block::{Block, ExecState, FullBlock};
use crate::common::executor::Executor;
use crate::common::mem_pool::MemPool;
use crate::common::network::Network;
use crate::common::storage::Storage;
use overlord::types::FullBlockWithProof;
use rlp::{Decodable, Encodable};

pub struct OverlordAdapter {
    network:  Arc<Network>,
    mem_pool: Arc<MemPool>,
    storage:  Arc<Storage>,

    address: Address,
}

impl OverlordAdapter {
    pub fn new(
        address: Address,
        network: &Arc<Network>,
        mem_pool: &Arc<MemPool>,
        storage: &Arc<Storage>,
    ) -> Self {
        let network = Arc::<Network>::clone(network);
        let mem_pool = Arc::<MemPool>::clone(mem_pool);
        let storage = Arc::<Storage>::clone(storage);

        OverlordAdapter {
            network,
            mem_pool,
            storage,
            address,
        }
    }
}

#[async_trait]
impl Adapter<Block, FullBlock, ExecState> for OverlordAdapter {
    type CryptoImpl = DefaultCrypto;

    async fn get_block_exec_result(
        &self,
        ctx: Context,
        height: Height,
    ) -> Result<ExecResult<ExecState>, Box<dyn Error + Send>> {
        let latest_height = self.storage.get_latest_height(&self.address);
        assert!(height <= latest_height);
        let blocks = self
            .storage
            .get_block_with_proof(&self.address, HeightRange::new(height, 1));
        let block = blocks[0].0.clone();
        let full_block = self.fetch_full_block(ctx, block).await?;
        Ok(Executor::exec(&full_block))
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_block(
        &self,
        _ctx: Context,
        height: Height,
        exec_height: Height,
        pre_hash: Hash,
        pre_proof: Proof,
        block_states: Vec<BlockState<ExecState>>,
        last_commit_exec_resp: ExecState,
    ) -> Result<Block, Box<dyn Error + Send>> {
        let mut state_root = last_commit_exec_resp.state_root;
        let receipt_roots: Vec<Hash> = block_states
            .iter()
            .map(|block_state| {
                state_root = block_state.state.state_root.clone();
                block_state.state.receipt_root.clone()
            })
            .collect();
        Ok(self.mem_pool.package(
            height,
            exec_height,
            pre_hash,
            pre_proof,
            state_root,
            receipt_roots,
        ))
    }

    async fn check_block(
        &self,
        _ctx: Context,
        block: Block,
        block_states: Vec<BlockState<ExecState>>,
        last_commit_exec_resp: ExecState,
    ) -> Result<(), Box<dyn Error + Send>> {
        let mut expect_state_root = last_commit_exec_resp.state_root;
        let expect_receipt_roots: Vec<Hash> = block_states
            .iter()
            .map(|block_state| {
                expect_state_root = block_state.state.state_root.clone();
                block_state.state.receipt_root.clone()
            })
            .collect();
        if expect_state_root != block.state_root {
            return Err(Box::new(BlockError(format!(
                "expect_state_root != block.state_root, {} != {}",
                expect_state_root.tiny_hex(),
                block.state_root.tiny_hex()
            ))));
        }
        if expect_receipt_roots != block.receipt_roots {
            return Err(Box::new(BlockError(format!(
                "expect_receipt_roots != block.receipt_roots, {:?} != {:?}",
                expect_receipt_roots
                    .iter()
                    .map(|r| r.tiny_hex())
                    .collect::<Vec<String>>(),
                block
                    .receipt_roots
                    .iter()
                    .map(|r| r.tiny_hex())
                    .collect::<Vec<String>>()
            ))));
        }
        Ok(())
    }

    async fn fetch_full_block(
        &self,
        _ctx: Context,
        block: Block,
    ) -> Result<FullBlock, Box<dyn Error + Send>> {
        let full_block = FullBlock { block };
        Ok(full_block)
    }

    async fn save_full_block_with_proof(
        &self,
        _ctx: Context,
        height: Height,
        full_block: FullBlock,
        proof: Proof,
        _is_sync: bool,
    ) -> Result<(), Box<dyn Error + Send>> {
        let block = full_block.get_block();
        self.storage
            .save_block_with_proof(self.address.clone(), height, block, proof);
        Ok(())
    }

    async fn exec_full_block(
        &self,
        _ctx: Context,
        _height: Height,
        full_block: FullBlock,
        _last_exec_resp: ExecState,
        _last_commit_exec_resp: ExecState,
    ) -> Result<ExecResult<ExecState>, Box<dyn Error + Send>> {
        Ok(Executor::exec(&full_block))
    }

    async fn commit(&self, _ctx: Context, _commit_state: ExecResult<ExecState>) {}

    async fn register_network(
        &self,
        _ctx: Context,
        sender: UnboundedSender<ChannelMsg<Block, FullBlock, ExecState>>,
    ) {
        self.network.register(self.address.clone(), sender);
    }

    async fn broadcast(
        &self,
        _ctx: Context,
        msg: OverlordMsg<Block, FullBlock>,
    ) -> Result<(), Box<dyn Error + Send>> {
        test_serialization(&msg);
        self.network.broadcast(&self.address, msg)
    }

    async fn transmit(
        &self,
        _ctx: Context,
        to: Address,
        msg: OverlordMsg<Block, FullBlock>,
    ) -> Result<(), Box<dyn Error + Send>> {
        test_serialization(&msg);
        self.network.transmit(&to, msg)
    }

    async fn get_full_block_with_proofs(
        &self,
        _ctx: Context,
        range: HeightRange,
    ) -> Result<Vec<FullBlockWithProof<Block, FullBlock>>, Box<dyn Error + Send>> {
        let block_with_proofs = self
            .storage
            .get_block_with_proof(&self.address, range)
            .into_iter()
            .map(|(block, proof)| {
                let full_block = FullBlock { block };
                FullBlockWithProof::new(proof, full_block)
            })
            .collect();
        Ok(block_with_proofs)
    }

    async fn get_latest_height(&self, _ctx: Context) -> Result<Height, Box<dyn Error + Send>> {
        Ok(self.storage.get_latest_height(&self.address))
    }

    async fn handle_error(&self, _ctx: Context, _err: OverlordError) {}
}

fn test_serialization(msg: &OverlordMsg<Block, FullBlock>) {
    match msg {
        OverlordMsg::SignedProposal(data) => {
            test_rlp(data);
        }
        OverlordMsg::SignedPreVote(data) => {
            test_rlp(data);
        }
        OverlordMsg::SignedPreCommit(data) => {
            test_rlp(data);
        }
        OverlordMsg::PreVoteQC(data) => {
            test_rlp(data);
        }
        OverlordMsg::PreCommitQC(data) => {
            test_rlp(data);
        }
        OverlordMsg::SignedChoke(data) => {
            test_rlp(data);
        }
        OverlordMsg::SignedHeight(data) => {
            test_rlp(data);
        }
        OverlordMsg::SyncRequest(data) => {
            test_rlp(data);
        }
        OverlordMsg::SyncResponse(data) => {
            test_rlp(data);
        }
        _ => {}
    }
}

fn test_rlp<T: Decodable + Encodable + PartialEq + Eq + std::fmt::Debug + std::fmt::Display>(
    data: &T,
) {
    let encode = rlp::encode(data);
    let decode: T =
        rlp::decode(&encode).unwrap_or_else(|e| panic!("decode error {} of {}", e, data));
    assert_eq!(data, &decode);
}

#[derive(Clone, Debug, Display)]
#[display(fmt = "block error: {}", _0)]
struct BlockError(String);

impl Error for BlockError {}
