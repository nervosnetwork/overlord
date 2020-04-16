#![allow(unused_imports)]
#![allow(dead_code)]

use std::error::Error;

use derive_more::Display;

pub type OverlordResult<T> = Result<T, OverlordError>;

#[derive(Debug, Display)]
pub enum ErrorKind {
    #[display(fmt = "Byzantine")]
    Byzantine,
    #[display(fmt = "Network")]
    Network,
    #[display(fmt = "Local")]
    Local,
    #[display(fmt = "Other")]
    Other,
}

#[derive(Debug, Display)]
pub enum ErrorInfo {
    #[display(fmt = "crypto error: {}", _0)]
    Crypto(Box<dyn Error + Send>),
    #[display(fmt = "unauthorized address")]
    UnAuthorized,
    #[display(fmt = "fake weight")]
    FakeWeight,
    #[display(fmt = "weight sum under majority")]
    UnderMajority,
}

#[derive(Debug, Display)]
#[display(fmt = "[OverlordError] Kind: {} Error: {}", kind, info)]
pub struct OverlordError {
    pub kind: ErrorKind,
    pub info: ErrorInfo,
}

impl Error for OverlordError {}
