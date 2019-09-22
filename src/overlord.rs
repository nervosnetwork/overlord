use std::marker::PhantomData;

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::StreamExt;
use log::error;
use parking_lot::RwLock;

use crate::error::ConsensusError;
use crate::state::process::State;
use crate::types::{Address, OverlordMsg};
use crate::{smr::SMRProvider, timer::Timer};
use crate::{Codec, Consensus, ConsensusResult, Crypto};

type Pile<T> = RwLock<Option<T>>;

/// An overlord consensus instance.
pub struct Overlord<T: Codec, S: Codec, F: Consensus<T, S>, C: Crypto> {
    sender:    Pile<UnboundedSender<(Context, OverlordMsg<T>)>>,
    state_rx:  Pile<UnboundedReceiver<(Context, OverlordMsg<T>)>>,
    address:   Pile<Address>,
    consensus: Pile<F>,
    crypto:    Pile<C>,
    pin_txs:   PhantomData<S>,
}

impl<T, S, F, C> Overlord<T, S, F, C>
where
    T: Codec + Send + Sync + 'static,
    S: Codec + Send + Sync + 'static,
    F: Consensus<T, S> + 'static,
    C: Crypto + Send + Sync + 'static,
{
    /// Create a new overlord and return an overlord instance with an unbounded receiver.
    pub fn new(address: Address, consensus: F, crypto: C) -> Self {
        let (tx, rx) = unbounded();
        Overlord {
            sender:    RwLock::new(Some(tx)),
            state_rx:  RwLock::new(Some(rx)),
            address:   RwLock::new(Some(address)),
            consensus: RwLock::new(Some(consensus)),
            crypto:    RwLock::new(Some(crypto)),
            pin_txs:   PhantomData,
        }
    }

    /// Get the overlord handler from the overlord instance.
    pub fn get_handler(&self) -> OverlordHandler<T> {
        let mut sender = self.sender.write();
        assert!(sender.is_some());
        OverlordHandler::new(sender.take().unwrap())
    }

    /// Run overlord consensus process. The `interval` is the epoch interval as millisecond.
    pub async fn run(&self, interval: u64) -> ConsensusResult<()> {
        let (mut smr_provider, evt_1, evt_2) = SMRProvider::new();
        let smr = smr_provider.take_smr();
        let mut timer = Timer::new(evt_2, smr.clone(), interval);

        let (rx, mut state) = {
            let mut state_rx = self.state_rx.write();
            let mut address = self.address.write();
            let mut consensus = self.consensus.write();
            let mut crypto = self.crypto.write();
            let sender = self.sender.read();

            let tmp_rx = state_rx.take().unwrap();
            let tmp_state = State::new(
                smr,
                address.take().unwrap(),
                interval,
                consensus.take().unwrap(),
                crypto.take().unwrap(),
            );

            assert!(sender.is_none());
            assert!(address.is_none());
            assert!(consensus.is_none());
            assert!(crypto.is_none());
            assert!(state_rx.is_none());

            (tmp_rx, tmp_state)
        };

        // Run SMR.
        smr_provider.run();

        // Run timer.
        runtime::spawn(async move {
            loop {
                timer.next().await;
            }
        });

        // Run state.
        state.run(rx, evt_1).await?;

        Ok(())
    }
}

/// An overlord handler to send messages to an overlord instance.
#[derive(Clone, Debug)]
pub struct OverlordHandler<T: Codec>(UnboundedSender<(Context, OverlordMsg<T>)>);

impl<T: Codec> OverlordHandler<T> {
    fn new(tx: UnboundedSender<(Context, OverlordMsg<T>)>) -> Self {
        OverlordHandler(tx)
    }

    /// Send overlord message to the instance. Return `Err()` when the message channel is closed.
    pub fn send_msg(&self, ctx: Context, msg: OverlordMsg<T>) -> ConsensusResult<()> {
        self.0
            .unbounded_send((ctx, msg))
            .map_err(|e| ConsensusError::Other(format!("Send message error {:?}", e)))
    }
}
