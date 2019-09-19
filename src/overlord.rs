use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::StreamExt;

use crate::error::ConsensusError;
use crate::state::process::State;
use crate::types::{Address, OverlordMsg};
use crate::{smr::SMRProvider, timer::Timer};
use crate::{Codec, Consensus, ConsensusResult, Crypto};

/// An overlord consensus instance.
pub struct Overlord<T: Codec, F: Consensus<T>, C: Crypto> {
    sender:    Option<UnboundedSender<(Context, OverlordMsg<T>)>>,
    state_rx:  Option<UnboundedReceiver<(Context, OverlordMsg<T>)>>,
    address:   Option<Address>,
    consensus: Option<F>,
    crypto:    Option<C>,
}

impl<T, F, C> Overlord<T, F, C>
where
    T: Codec + Send + Sync + 'static,
    F: Consensus<T> + 'static,
    C: Crypto + Send + Sync + 'static,
{
    /// Create a new overlord and return an overlord instance with an unbounded receiver.
    pub fn new(address: Address, consensus: F, crypto: C) -> Self {
        let (tx, rx) = unbounded();
        Overlord {
            sender:    Some(tx),
            state_rx:  Some(rx),
            address:   Some(address),
            consensus: Some(consensus),
            crypto:    Some(crypto),
        }
    }

    /// Take the overlord handler from the overlord instance.
    pub fn take_handler(&mut self) -> OverlordHandler<T> {
        assert!(self.sender.is_some());
        OverlordHandler::new(self.sender.take().unwrap())
    }

    /// Run overlord consensus process. The `interval` is the epoch interval as millisecond.
    pub fn run(mut self, interval: u64) {
        let (mut smr_provider, evt_1, evt_2) = SMRProvider::new();
        let smr = smr_provider.take_smr();
        let mut timer = Timer::new(evt_2, smr.clone(), interval);
        let state_rx = self.state_rx.take().unwrap();
        let mut state = State::new(
            smr,
            self.address.take().unwrap(),
            interval,
            self.consensus.take().unwrap(),
            self.crypto.take().unwrap(),
        );

        assert!(self.sender.is_none());
        assert!(self.address.is_none());
        assert!(self.consensus.is_none());
        assert!(self.crypto.is_none());
        assert!(self.state_rx.is_none());

        // Run SMR.
        smr_provider.run();

        // Run timer.
        tokio::spawn(async move {
            loop {
                timer.next().await;
            }
        });

        // Run state.
        tokio::spawn(async move {
            let _ = state.run(state_rx, evt_1).await;
        });
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
