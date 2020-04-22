use overlord::{Blk, BlockState, ExecResult};

use crate::common::block::{ExecState, FullBlock};

pub struct Executor;

impl Executor {
    pub fn exec(full_block: &FullBlock) -> ExecResult<ExecState> {
        let consensus_config = full_block.block.tx.clone();
        let hash = full_block.block.get_block_hash().unwrap();
        let exec_state = ExecState {
            state_root:   hash.clone(),
            receipt_root: hash,
        };
        let block_states = BlockState {
            height: full_block.block.height,
            state:  exec_state,
        };
        ExecResult {
            consensus_config,
            block_states,
        }
    }
}
