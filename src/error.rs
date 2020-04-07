use std::error::Error;
use std::result::Result;

use derive_more::Display;

pub type ConsensusResult<T> = Result<T, ConsensusError>;

#[derive(Debug, Display)]
pub enum ConsensusError {
    #[display(fmt = "Other Error {}", _0)]
    Other(String),
}

impl Error for ConsensusError {}
