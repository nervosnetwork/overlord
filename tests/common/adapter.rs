use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use creep::Context;
use futures::channel::mpsc::UnboundedSender;
use overlord::{
    Adapter, Address, BlockState, DefaultCrypto, ExecResult, Hash, Height, HeightRange,
    OverlordError, OverlordMsg, Proof,
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

    address:         Address,
    last_state_root: Hash,
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
        let last_state_root = Hash::default();

        OverlordAdapter {
            network,
            mem_pool,
            storage,
            address,
            last_state_root,
        }
    }
}

#[async_trait]
impl Adapter<Block, ExecState> for OverlordAdapter {
    type CryptoImpl = DefaultCrypto;

    async fn create_block(
        &self,
        _ctx: Context,
        height: Height,
        exec_height: Height,
        pre_hash: Hash,
        pre_proof: Proof,
        block_states: Vec<BlockState<ExecState>>,
    ) -> Result<Block, Box<dyn Error + Send>> {
        let mut state_root = self.last_state_root.clone();
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
    ) -> Result<(), Box<dyn Error + Send>> {
        let mut expect_state_root = self.last_state_root.clone();
        let expect_receipt_roots: Vec<Hash> = block_states
            .iter()
            .map(|block_state| {
                expect_state_root = block_state.state.state_root.clone();
                block_state.state.receipt_root.clone()
            })
            .collect();
        assert_eq!(expect_state_root, block.state_root);
        assert_eq!(expect_receipt_roots, block.receipt_roots);
        Ok(())
    }

    async fn fetch_full_block(
        &self,
        _ctx: Context,
        block: &Block,
    ) -> Result<Bytes, Box<dyn Error + Send>> {
        let full_block = FullBlock {
            block: block.clone(),
        };
        let vec = bincode::serialize(&full_block).unwrap();
        Ok(Bytes::from(vec))
    }

    async fn save_and_exec_block_with_proof(
        &self,
        _ctx: Context,
        height: Height,
        full_block: Bytes,
        proof: Proof,
    ) -> Result<ExecResult<ExecState>, Box<dyn Error + Send>> {
        let full_block: FullBlock = bincode::deserialize(&full_block).unwrap();
        let block = full_block.block.clone();
        self.storage
            .save_block_with_proof(self.address.clone(), height, block, proof);
        Ok(Executor::exec(&full_block))
    }

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
