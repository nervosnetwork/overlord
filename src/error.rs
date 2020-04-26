use std::error::Error;

use derive_more::Display;
use rlp::DecoderError;

use crate::{Hash, TinyHex};

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
    #[display(fmt = "fake vote weight, {}", _0)]
    FakeWeight(String),
    #[display(fmt = "weight sum under majority")]
    UnderMajority,
    #[display(fmt = "msg already exist")]
    MsgExist,
    #[display(fmt = "msg of multi version, {}", _0)]
    MultiVersion(String),
    #[display(fmt = "get block failed: {}", _0)]
    GetBlock(Box<dyn Error + Send>),
    #[display(fmt = "fetch full block of {} failed", "_0.tiny_hex()")]
    FetchFullBlock(Hash),
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
    #[display(fmt = "receive msg with wrong leader, {}", _0)]
    WrongLeader(String),
    #[display(fmt = "check block failed, {}", _0)]
    CheckBlock(String),
    #[display(fmt = "adapter check block failed, {}", _0)]
    AdapterCheckBlock(Box<dyn Error + Send>),
    #[display(fmt = "under stage")]
    UnderStage,
    #[display(fmt = "transmit error, {}", _0)]
    Transmit(Box<dyn Error + Send>),
    #[display(fmt = "broadcast error, {}", _0)]
    Broadcast(Box<dyn Error + Send>),
    #[display(fmt = "create block error, {}", _0)]
    CreateBlock(Box<dyn Error + Send>),
    #[display(fmt = "lock empty vote in proposal")]
    EmptyLock,
    #[display(fmt = "exec behind consensus too much, {}", _0)]
    BehindMuch(String),
    #[display(fmt = "sender is in black list")]
    InBlackList,
    #[display(fmt = "self on sync")]
    OnSync,
    #[display(fmt = "request higher block, {}", _0)]
    RequestHigher(String),
    #[display(fmt = "not ready to sync, {}", _0)]
    NotReady(String),
    #[display(fmt = "sync error, {}", _0)]
    Sync(String),
    #[display(fmt = "hash block error, {}", _0)]
    HashBlock(Box<dyn Error + Send>),
    #[display(fmt = "wait for full block")]
    WaitFullBlock,
}

#[derive(Debug, Display)]
#[display(fmt = "<{}> {}", kind, info)]
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

    pub fn debug_un_auth() -> Self {
        OverlordError {
            kind: ErrorKind::Debug,
            info: ErrorInfo::UnAuthorized,
        }
    }

    pub fn byz_fake(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::FakeWeight(str),
        }
    }

    pub fn byz_under_maj() -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::UnderMajority,
        }
    }

    pub fn debug_msg_exist() -> Self {
        OverlordError {
            kind: ErrorKind::Debug,
            info: ErrorInfo::MsgExist,
        }
    }

    pub fn byz_mul_version(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::MultiVersion(str),
        }
    }

    pub fn local_get_block(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::GetBlock(e),
        }
    }

    pub fn net_fetch(hash: Hash) -> Self {
        OverlordError {
            kind: ErrorKind::Network,
            info: ErrorInfo::FetchFullBlock(hash),
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

    pub fn byz_leader(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::WrongLeader(str),
        }
    }

    pub fn byz_block(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::CheckBlock(str),
        }
    }

    pub fn warn_block(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::Warn,
            info: ErrorInfo::CheckBlock(str),
        }
    }

    pub fn debug_under_stage() -> Self {
        OverlordError {
            kind: ErrorKind::Debug,
            info: ErrorInfo::UnderStage,
        }
    }

    pub fn net_transmit(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::Network,
            info: ErrorInfo::Transmit(e),
        }
    }

    pub fn local_broadcast(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::Broadcast(e),
        }
    }

    pub fn byz_adapter_check_block(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::AdapterCheckBlock(e),
        }
    }

    pub fn local_create_block(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::CreateBlock(e),
        }
    }

    pub fn byz_empty_lock() -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::EmptyLock,
        }
    }

    pub fn local_behind(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::LocalError,
            info: ErrorInfo::BehindMuch(str),
        }
    }

    pub fn net_blacklist() -> Self {
        OverlordError {
            kind: ErrorKind::Network,
            info: ErrorInfo::InBlackList,
        }
    }

    pub fn net_on_sync() -> Self {
        OverlordError {
            kind: ErrorKind::Network,
            info: ErrorInfo::OnSync,
        }
    }

    pub fn debug_on_sync() -> Self {
        OverlordError {
            kind: ErrorKind::Debug,
            info: ErrorInfo::OnSync,
        }
    }

    pub fn byz_req_high(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::RequestHigher(str),
        }
    }

    pub fn debug_not_ready(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::Debug,
            info: ErrorInfo::NotReady(str),
        }
    }

    pub fn byz_sync(str: String) -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::Sync(str),
        }
    }

    pub fn byz_hash(e: Box<dyn Error + Send>) -> Self {
        OverlordError {
            kind: ErrorKind::Byzantine,
            info: ErrorInfo::HashBlock(e),
        }
    }

    pub fn warn_wait() -> Self {
        OverlordError {
            kind: ErrorKind::Warn,
            info: ErrorInfo::WaitFullBlock,
        }
    }
}

impl Error for OverlordError {}
