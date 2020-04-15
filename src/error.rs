#![allow(unused_imports)]
#![allow(dead_code)]

use std::error::Error;

use derive_more::Display;

#[derive(Clone, Debug)]
pub enum OverlordErrorKind {
    Byzantine,
    RepeatMsg,
    Other,
}

#[derive(Debug, Display)]
#[display(fmt = "[ProtocolError] Kind: {:?} Error: {:?}", kind, error)]
pub struct OverlordError {
    kind:  OverlordErrorKind,
    error: Box<dyn Error + Send>,
}

impl Error for OverlordError {}
