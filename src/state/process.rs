use std::collections::HashMap;
use std::ops::BitXor;

use bytes::Bytes;
use rlp::encode;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::smr::smr_types::{SMRTrigger, TriggerSource, TriggerType};
use crate::smr::{Event, SMR};
use crate::state::collection::{ProposalCollector, VoteCollector};
use crate::types::{
    Address, Hash, OverlordMsg, PoLC, Proof, Proposal, Signature, SignedProposal, VoteType,
};
use crate::{error::ConsensusError, utils::auth_manage::AuthorityManage};
use crate::{Codec, Consensus, ConsensusResult, Crypto, INIT_EPOCH_ID, INIT_ROUND};

/// **TODO: context libiary**
const CTX: u8 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
enum MsgType {
    SignedProposal,
    SignedVote,
}

///
#[derive(Debug)]
pub struct Overlord<T: Codec, F: Consensus<T>, C: Crypto> {
    rx:    UnboundedReceiver<OverlordMsg<T>>,
    event: Event,

    epoch_id:             u64,
    round:                u64,
    state_machine:        SMR,
    address:              Address,
    proposals:            ProposalCollector<T>,
    votes:                VoteCollector,
    authority:            AuthorityManage,
    hash_with_epoch:      HashMap<Hash, T>,
    is_leader:            bool,
    proof:                Option<Proof>,
    last_commit_round:    Option<u64>,
    last_commit_proposal: Option<u64>,

    function: F,
    util:     C,
}

impl<T, F, C> Overlord<T, F, C>
where
    T: Codec,
    F: Consensus<T>,
    C: Crypto,
{
    pub fn new(
        receiver: UnboundedReceiver<OverlordMsg<T>>,
        monitor: Event,
        smr: SMR,
        addr: Address,
        consensus: F,
        crypto: C,
    ) -> Self {
        Overlord {
            rx:    receiver,
            event: monitor,

            epoch_id:             INIT_EPOCH_ID,
            round:                INIT_ROUND,
            state_machine:        smr,
            address:              addr,
            proposals:            ProposalCollector::new(),
            votes:                VoteCollector::new(),
            authority:            AuthorityManage::new(),
            hash_with_epoch:      HashMap::new(),
            is_leader:            false,
            proof:                None,
            last_commit_round:    None,
            last_commit_proposal: None,

            function: consensus,
            util:     crypto,
        }
    }

    /// Handle `NewRoundInfo` event from SMR. Firstly, goto new round and check the `XOR`
    /// relationship between the lock round type and the lock proposal type. Secondly, check if self
    /// is a proposer. If is not a proposer, return `Ok(())` and wait for a signed proposal from the
    /// network. Otherwise, make up a proposal, broadcast it and touch off SMR trigger.
    async fn handle_new_round(
        &mut self,
        round: u64,
        lock_round: Option<u64>,
        lock_proposal: Option<Hash>,
    ) -> ConsensusResult<()> {
        self.round = round;
        self.is_leader = false;

        if lock_round.is_some().bitxor(lock_proposal.is_some()) {
            return Err(ConsensusError::ProposalErr(
                "Lock round is inconsistent with lock proposal".to_string(),
            ));
        }

        // If self is not proposer, check whether it has received current signed proposal before. If
        // has, then handle it.
        if !self.is_proposer()? {
            if let Ok(signed_proposal) = self.proposals.get(self.epoch_id, self.round) {
                return self.handle_signed_proposal(signed_proposal);
            }
            return Ok(());
        }

        // There two cases to be handle when package a proposal:
        //
        // 1. Proposal without a lock
        // If the lock round field of `NewRoundInfo` event from SMR is none, state should get a new
        // epoch with its hash. These things consititute a Proposal. Then sign it and broadcast it
        // to other nodes.
        //
        // 2. Proposal with a lock
        // The case is much more complex. State should get the whole epoch and prevote quorum
        // certificate form proposal collector and vote collector. Some necessary checks should be
        // done by doing this. These things consititute a Proposal. Then sign it and broadcast it to
        // other nodes.
        self.is_leader = true;
        let (epoch, hash, polc) = if lock_round.is_none() {
            let (new_epoch, new_hash) = self
                .function
                .get_epoch(vec![CTX], self.epoch_id)
                .await
                .map_err(|err| ConsensusError::Other(format!("{:?}", err)))?;
            (new_epoch, new_hash, None)
        } else {
            let round = lock_round.clone().unwrap();
            let hash = lock_proposal.unwrap();
            let epoch = self.hash_with_epoch.get(&hash).ok_or_else(|| {
                ConsensusError::ProposalErr(format!("Lose whole epoch that hash is {:?}", hash))
            })?;

            // Create PoLC by prevoteQC.
            let qc = self
                .votes
                .get_qc(self.epoch_id, round, VoteType::Prevote)
                .map_err(|err| ConsensusError::ProposalErr(format!("{:?} when propose", err)))?;
            let polc = PoLC {
                lock_round: round,
                lock_votes: qc,
            };
            (epoch.to_owned(), hash, Some(polc))
        };

        let proposal = Proposal {
            epoch_id:   self.epoch_id,
            round:      self.round,
            content:    epoch,
            epoch_hash: hash.clone(),
            lock:       polc,
            proposer:   self.address.clone(),
        };

        // **TODO: parallelism**
        self.function
            .broadcast_to_other(
                vec![CTX],
                OverlordMsg::SignedProposal(self.sign_proposal(proposal)?),
            )
            .await
            .map_err(|err| ConsensusError::Other(format!("{:?}", err)))?;

        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::Proposal,
            source: TriggerSource::State,
            hash,
            round: lock_round,
        })?;
        Ok(())
    }

    /// This function only handle signed proposals which epoch ID and round are equal to current.
    /// Others will be ignored or storaged in the proposal collector.
    fn handle_signed_proposal(
        &mut self,
        signed_proposal: SignedProposal<T>,
    ) -> ConsensusResult<()> {
        let epoch_id = signed_proposal.proposal.epoch_id;
        let round = signed_proposal.proposal.round;

        // If the proposal epoch ID is lower than the current epoch ID - 1, or the proposal epoch ID
        // is equal to the current epoch ID and the proposal round is lower than the current round,
        // ignore it directly.
        if epoch_id < self.epoch_id - 1 || (epoch_id == self.epoch_id && round < self.round) {
            return Ok(());
        }

        // If the proposal epoch ID is higher than the current epoch ID or proposal epoch ID is
        // equal to the current epoch ID and the proposal round is higher than self lock round the
        // current round, cache it until that epoch ID.
        if epoch_id > self.epoch_id || (epoch_id == self.epoch_id && round > self.round) {
            self.proposals.insert(epoch_id, round, signed_proposal)?;
            return Ok(());
        }

        // TODO: handle proposal.epoch_id == self.epoch_id - 1 && proposal.round > self.round

        //  Verify proposal signature.
        let proposal = signed_proposal.proposal;
        let signature = signed_proposal.signature;
        self.verify_signature(
            self.util.hash(Bytes::from(encode(&proposal))),
            signature,
            &proposal.proposer,
            MsgType::SignedProposal,
        )?;

        // If the signed proposal is with a lock, check the lock round and the QC then trigger it to
        // SMR. Otherwise, touch off SMR directly.
        let lock_round = if let Some(polc) = proposal.lock.clone() {
            if !self.authority.is_above_threshold(
                polc.lock_votes.signature.address_bitmap.clone(),
                proposal.epoch_id == self.epoch_id,
            )? {
                return Err(ConsensusError::AggregatedSignatureErr(format!(
                    "Aggregated signature below two thrids, proposal of epoch ID {:?}, round {:?}",
                    proposal.epoch_id, proposal.round
                )));
            }

            self.util
                .verify_aggregated_signature(polc.lock_votes.signature)
                .map_err(|err| {
                    ConsensusError::AggregatedSignatureErr(format!(
                        "{:?} proposal of epoch ID {:?}, round {:?}",
                        err, proposal.epoch_id, proposal.round
                    ))
                })?;
            Some(polc.lock_round)
        } else {
            None
        };

        self.hash_with_epoch
            .insert(proposal.epoch_hash.clone(), proposal.content);
        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::Proposal,
            source:       TriggerSource::State,
            hash:         proposal.epoch_hash,
            round:        lock_round,
        })?;
        Ok(())
    }

    fn is_proposer(&self) -> ConsensusResult<bool> {
        Ok(self
            .authority
            .get_proposer(self.epoch_id + self.round, true)?
            == self.address)
    }

    fn sign_proposal(&self, proposal: Proposal<T>) -> ConsensusResult<SignedProposal<T>> {
        let signature = self
            .util
            .sign(self.util.hash(Bytes::from(encode(&proposal))))
            .map_err(|err| ConsensusError::CryptoErr(format!("{:?}", err)))?;

        Ok(SignedProposal {
            signature,
            proposal,
        })
    }

    fn verify_signature(
        &self,
        hash: Hash,
        signature: Signature,
        address: &Address,
        msg_type: MsgType,
    ) -> ConsensusResult<()> {
        let addr = self.util.verify_signature(signature, hash).map_err(|err| {
            ConsensusError::CryptoErr(format!("{:?} signature error {:?}", msg_type, err))
        })?;

        if address != &addr {
            return Err(ConsensusError::CryptoErr(format!(
                "{:?} signature wrong",
                msg_type
            )));
        }
        Ok(())
    }
}
