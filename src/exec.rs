#![allow(unused_imports)]
#![allow(unused_variables)]

use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;

use bytes::Bytes;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::stream::{Stream, StreamExt};
use futures::{FutureExt, SinkExt};
use futures_timer::Delay;
use log::error;

use crate::auth::{AuthFixedConfig, AuthManage};
use crate::cabinet::Cabinet;
use crate::state::{Stage, StateError, StateInfo};
use crate::timeout::TimeoutEvent;
use crate::types::{FetchedFullBlock, Proposal, UpdateFrom};
use crate::{
    Adapter, Address, Blk, BlockState, CommonHex, Height, HeightRange, OverlordMsg, PriKeyHex,
    Proof, Round, St, TimeConfig, Wal,
};

pub struct ExecRequest {
    height:     Height,
    full_block: Bytes,
    proof:      Proof,
}

pub struct Exec<A: Adapter<B, S>, B: Blk, S: St> {
    adapter: Arc<A>,
    queue:   UnboundedReceiver<ExecRequest>,

    phantom_b: PhantomData<B>,
    phantom_s: PhantomData<S>,
}
