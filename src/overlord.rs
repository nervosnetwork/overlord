use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use parking_lot::RwLock;

use crate::error::ConsensusError;
use crate::state::process::State;
use crate::types::{Address, Node, OverlordMsg};
use crate::DurationConfig;
#[cfg(feature = "log_prefix")]
use crate::{info, LOG_PREFIX};
use crate::{smr::SMR, timer::Timer};
use crate::{Codec, Consensus, ConsensusResult, Crypto, Wal};
#[cfg(not(feature = "log_prefix"))]
use log::info;

type Pile<T> = RwLock<Option<T>>;

/// An overlord consensus instance.
pub struct Overlord<T: Codec, F: Consensus<T>, C: Crypto, W: Wal> {
    sender:    Pile<UnboundedSender<(Context, OverlordMsg<T>)>>,
    state_rx:  Pile<UnboundedReceiver<(Context, OverlordMsg<T>)>>,
    address:   Pile<Address>,
    consensus: Pile<Arc<F>>,
    crypto:    Pile<Arc<C>>,
    wal:       Pile<Arc<W>>,
}

impl<T, F, C, W> Overlord<T, F, C, W>
where
    T: Codec + Send + Sync + 'static,
    F: Consensus<T> + 'static,
    C: Crypto + Send + Sync + 'static,
    W: Wal + 'static,
{
    /// Create a new overlord and return an overlord instance with an unbounded receiver.
    pub fn new(
        address: Address,
        consensus: Arc<F>,
        crypto: Arc<C>,
        wal: Arc<W>,
        #[cfg(feature = "log_prefix")] log_prefix: &str,
    ) -> Self {
        #[cfg(feature = "log_prefix")]
        {
            *LOG_PREFIX.write() = String::from(log_prefix);
        }

        let (tx, rx) = unbounded();
        Overlord {
            sender:    RwLock::new(Some(tx)),
            state_rx:  RwLock::new(Some(rx)),
            address:   RwLock::new(Some(address)),
            consensus: RwLock::new(Some(consensus)),
            crypto:    RwLock::new(Some(crypto)),
            wal:       RwLock::new(Some(wal)),
        }
    }

    /// Get the overlord handler from the overlord instance.
    pub fn get_handler(&self) -> OverlordHandler<T> {
        let sender = self.sender.write();
        assert!(sender.is_some());
        let tx = sender.clone().unwrap();
        OverlordHandler::new(tx)
    }

    /// Run overlord consensus process. The `interval` is the height interval as millisecond.
    pub async fn run(
        &self,
        interval: u64,
        authority_list: Vec<Node>,
        timer_config: Option<DurationConfig>,
    ) -> ConsensusResult<()> {
        let (mut smr_provider, evt_state, evt_timer) = SMR::new();
        let smr_handler = smr_provider.take_smr();
        let timer = Timer::new(evt_timer, smr_handler.clone(), interval, timer_config);

        let (rx, mut state, resp) = {
            let mut state_rx = self.state_rx.write();
            let mut address = self.address.write();
            let mut consensus = self.consensus.write();
            let mut crypto = self.crypto.write();
            let mut wal = self.wal.write();
            // let sender = self.sender.read();

            let tmp_rx = state_rx.take().unwrap();
            let (tmp_state, tmp_resp) = State::new(
                smr_handler,
                address.take().unwrap(),
                interval,
                authority_list,
                consensus.take().unwrap(),
                crypto.take().unwrap(),
                wal.take().unwrap(),
            );

            // assert!(sender.is_none());
            assert!(address.is_none());
            assert!(consensus.is_none());
            assert!(crypto.is_none());
            assert!(state_rx.is_none());
            assert!(wal.is_none());

            (tmp_rx, tmp_state, tmp_resp)
        };

        info!("Overlord start running");

        // Run SMR.
        smr_provider.run();

        // Run timer.
        timer.run();

        // Run state.
        state.run(rx, evt_state, resp).await;

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
        if self.0.is_closed() {
            log::error!("[OverlordHandler]: channel closed");
            Ok(())
        } else {
            self.0
                .unbounded_send((ctx, msg))
                .map_err(|e| ConsensusError::Other(format!("Send message error {:?}", e)))
        }
    }
}
