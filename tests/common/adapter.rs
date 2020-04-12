use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use creep::Context;
use futures::channel::mpsc::UnboundedSender;
use overlord::{
    Adapter, Address, BlockState, ConsensusError, ExecResult, Hash, Height, HeightRange,
    OverlordMsg, Proof,
};

use crate::common::block::{Block, ExecState};
use crate::common::network::Network;

pub struct OverlordAdapter {
    address: Address,
    network: Arc<Network>,
    // storage: Arc<>,
}

impl OverlordAdapter {
    pub fn new(address: Address, network: Arc<Network>) -> Self {
        OverlordAdapter { address, network }
    }
}

#[async_trait]
impl Adapter<Block, ExecState> for OverlordAdapter {
    async fn create_block(
        &self,
        _ctx: Context,
        _height: Height,
        _pre_exec_height: Height,
        _pre_hash: Hash,
        _pre_proof: Proof,
        _block_states: Vec<BlockState<ExecState>>,
    ) -> Result<Block, Box<dyn Error + Send>> {
        Ok(Block::default())
    }

    async fn check_block(
        &self,
        _ctx: Context,
        _height: Height,
        _pre_exec_height: Height,
        _block: Block,
        _block_states: Vec<BlockState<ExecState>>,
    ) -> Result<(), Box<dyn Error + Send>> {
        Ok(())
    }

    async fn exec_block(
        &self,
        _ctx: Context,
        _height: Height,
        _block: Block,
    ) -> Result<ExecResult<ExecState>, Box<dyn Error + Send>> {
        Ok(ExecResult::default())
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

    async fn get_blocks(
        &self,
        _ctx: Context,
        _height_range: HeightRange,
    ) -> Result<Vec<(Block, Proof)>, Box<dyn Error + Send>> {
        Ok(vec![])
    }

    async fn get_last_exec_height(&self, _ctx: Context) -> Result<Height, Box<dyn Error + Send>> {
        Ok(0)
    }

    async fn register_network(&self, _ctx: Context, sender: UnboundedSender<OverlordMsg<Block>>) {
        self.network.register(self.address.clone(), sender);
    }

    async fn handle_error(&self, _ctx: Context, _err: ConsensusError) {}
}
