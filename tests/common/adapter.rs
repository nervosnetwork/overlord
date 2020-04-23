use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::UnboundedSender;
use overlord::{
    Adapter, Address, BlockState, DefaultCrypto, ExecResult, Hash, Height, HeightRange,
    OverlordError, OverlordMsg, Proof, TinyHex,
};

use crate::common::block::{Block, ExecState, FullBlock};
use crate::common::executor::Executor;
use crate::common::mem_pool::MemPool;
use crate::common::network::Network;
use crate::common::storage::Storage;

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
impl Adapter<Block, ExecState> for OverlordAdapter {
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
        let full_block: FullBlock =
            bincode::deserialize(&full_block).expect("deserialize full block failed");
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
        block: &Block,
        block_states: &[BlockState<ExecState>],
        last_commit_exec_resp: &ExecState,
    ) -> Result<(), Box<dyn Error + Send>> {
        let mut expect_state_root = last_commit_exec_resp.state_root.clone();
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
    ) -> Result<Bytes, Box<dyn Error + Send>> {
        let full_block = FullBlock { block };
        let vec = bincode::serialize(&full_block).expect("serialize full block failed");
        Ok(Bytes::from(vec))
    }

    async fn save_and_exec_block_with_proof(
        &self,
        _ctx: Context,
        height: Height,
        full_block: Bytes,
        proof: Proof,
        _last_exec_resp: ExecState,
        _last_commit_exec_resp: ExecState,
    ) -> Result<ExecResult<ExecState>, Box<dyn Error + Send>> {
        let full_block: FullBlock =
            bincode::deserialize(&full_block).expect("deserialize full block failed");
        let block = full_block.block.clone();
        self.storage
            .save_block_with_proof(self.address.clone(), height, block, proof);
        Ok(Executor::exec(&full_block))
    }

    async fn commit(&self, _ctx: Context, _commit_state: ExecResult<ExecState>) {}

    async fn register_network(
        &self,
        _ctx: Context,
        sender: UnboundedSender<(Context, OverlordMsg<Block>)>,
    ) {
        self.network.register(self.address.clone(), sender);
    }

    async fn broadcast(
        &self,
        _ctx: Context,
        msg: OverlordMsg<Block>,
    ) -> Result<(), Box<dyn Error + Send>> {
        self.network.broadcast(&self.address, msg)
    }

    async fn transmit(
        &self,
        _ctx: Context,
        to: Address,
        msg: OverlordMsg<Block>,
    ) -> Result<(), Box<dyn Error + Send>> {
        self.network.transmit(&to, msg)
    }

    async fn get_block_with_proofs(
        &self,
        _ctx: Context,
        range: HeightRange,
    ) -> Result<Vec<(Block, Proof)>, Box<dyn Error + Send>> {
        Ok(self.storage.get_block_with_proof(&self.address, range))
    }

    async fn get_latest_height(&self, _ctx: Context) -> Result<Height, Box<dyn Error + Send>> {
        Ok(self.storage.get_latest_height(&self.address))
    }

    async fn handle_error(&self, _ctx: Context, _err: OverlordError) {}
}

#[derive(Clone, Debug, Display)]
#[display(fmt = "block error: {}", _0)]
struct BlockError(String);

impl Error for BlockError {}
