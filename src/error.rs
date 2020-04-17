#![allow(unused_imports)]
#![allow(dead_code)]

use std::error::Error;

use derive_more::Display;
use rlp::DecoderError;

use crate::cabinet::Capsule;
use crate::{Address, Blk, Height};

pub type OverlordResult<T> = Result<T, OverlordError>;

#[derive(Debug, Display)]
pub enum ErrorKind {
    #[display(fmt = "Byzantine")]
    Byzantine,
    #[display(fmt = "Network")]
    Network,
    #[display(fmt = "LocalError")]
    LocalError,
    #[display(fmt = "Warn")]
    Warn,
    #[display(fmt = "Debug")]
    Debug,
}

#[derive(Debug, Display)]
pub enum ErrorInfo {
    #[display(fmt = "crypto error: {}", _0)]
    Crypto(Box<dyn Error + Send>),
    #[display(fmt = "unauthorized")]
    UnAuthorized,
    #[display(fmt = "fake weight")]
    FakeWeight,
    #[display(fmt = "weight sum under majority")]
    UnderMajority,
    #[display(fmt = "msg already exist")]
    MsgExist,
    #[display(fmt = "msg of multi version")]
    MultiVersion,
    #[display(fmt = "get block failed: {}", _0)]
    GetBlock(Box<dyn Error + Send>),
    #[display(fmt = "fetch full block failed {}", _0)]
    FetchFullBlock(Box<dyn Error + Send>),
    #[display(fmt = "exec block failed {}", _0)]
    Exec(Box<dyn Error + Send>),
    #[display(fmt = "operate wal file failed, {:?}", _0)]
    WalFile(std::io::Error),
    #[display(fmt = "other error, {}", _0)]
    Other(String),
    #[display(fmt = "decode error, {:?}", _0)]
    Decode(DecoderError),
    #[display(fmt = "receive old msg")]
    OldMsg,
    #[display(fmt = "receive much higher msg")]
    MuchHighMsg,
    #[display(fmt = "receive higher msg")]
    HighMsg,
    #[display(fmt = "receive msg with wrong leader")]
    WrongLeader,
    #[display(fmt = "check block failed")]
    CheckBlock,
}

#[derive(Debug, Display)]
#[display(fmt = "[OverlordError] Kind: {} Error: {}", kind, info)]
pub struct OverlordError {
    pub kind: ErrorKind,
    pub info: ErrorInfo,
}

impl OverlordError {
    pub fn byz_crypto(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::Crypto(e),
        }
    }

    pub fn local_crypto(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::Crypto(e),
        }
    }

    pub fn byz_un_auth() -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::UnAuthorized,
        }
    }

    pub fn byz_fake() -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::FakeWeight,
        }
    }

    pub fn byz_under_maj() -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::UnderMajority,
        }
    }

    pub fn net_msg_exist() -> Self {
        OverlordError {
            kind: ErrorKind::Network,
            info: ErrorInfo::MsgExist,
        }
    }

    pub fn byz_mul_version() -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::MultiVersion,
        }
    }

    pub fn local_get_block(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::GetBlock(e),
        }
    }

    pub fn net_fetch(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::Network,
            info: ErrorInfo::FetchFullBlock(e),
        }
    }

    pub fn local_exec(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::Exec(e),
        }
    }

    pub fn local_wal(e: std::io::Error) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::WalFile(e),
        }
    }

    pub fn local_decode(e: DecoderError) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::Decode(e),
        }
    }

    pub fn local_other(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::Other(str),
        }
    }

    pub fn debug_old() -> Self {
        OverlordError {
            kind: ErrorKind::Debug,
            info: ErrorInfo::OldMsg,
        }
    }

    pub fn debug_high() -> Self {
        OverlordError {
            kind: ErrorKind::Debug,
            info: ErrorInfo::HighMsg,
        }
    }

    pub fn net_much_high() -> Self {
        OverlordError {
            kind: ErrorKind::Network,
            info: ErrorInfo::MuchHighMsg,
        }
    }

    pub fn byz_leader() -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::WrongLeader,
        }
    }

    pub fn byz_block() -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::CheckBlock,
        }
    }

    pub fn warn_block() -> Self {
        OverlordError {
            kind: ErrorKind::Warn,
            info: ErrorInfo::CheckBlock,
        }
    }
}

impl Error for OverlordError {}
