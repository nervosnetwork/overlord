use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::time::{Duration, Instant};
use std::{ops::BitXor, sync::Arc};

use bit_vec::BitVec;
use bytes::Bytes;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{unbounded, UnboundedReceiver};
use futures::{select, StreamExt};
use futures_timer::Delay;
use log::{debug, error, info, warn};
use parking_lot::Mutex;
use rlp::encode;

use crate::smr::smr_types::{SMREvent, SMRTrigger, TriggerSource, TriggerType};
use crate::smr::{Event, SMR};
use crate::state::collection::{ProposalCollector, VoteCollector};
use crate::types::{
    Address, AggregatedSignature, AggregatedVote, Commit, Hash, OverlordMsg, PoLC, Proof, Proposal,
    Signature, SignedProposal, SignedVote, Status, Vote, VoteType,
};
use crate::{error::ConsensusError, utils::auth_manage::AuthorityManage};
use crate::{Codec, Consensus, ConsensusResult, Crypto, INIT_EPOCH_ID, INIT_ROUND};

#[cfg(feature = "test")]
use {
    crate::utils::metrics::{metrics_enabled, timestamp},
    crate::{metrics, smr::smr_types::Step, utils::timestamp::Timestamp},
    serde_json::json,
};

// #[cfg(test)]
// use crate::types::Node;

const CHECK_EPOCH_FLAG: bool = true;

#[derive(Clone, Debug, Display, PartialEq, Eq)]
enum MsgType {
    #[display(fmt = "Signed Proposal message")]
    SignedProposal,
    #[display(fmt = "Signed Vote message")]
    SignedVote,
}

/// Overlord state struct. It maintains the local state of the node, and monitor the SMR event. The
/// `proposals` is used to cache the signed proposals that are with higher epoch ID or round. The
/// `hash_with_epoch` field saves hash and its corresponding epoch with the current epoch ID and
/// round. The `votes` field saves all signed votes and quorum certificates which epoch ID is higher
/// than `current_epoch - 1`.
#[derive(Debug)]
pub struct State<T: Codec, S: Codec, F: Consensus<T, S>, C: Crypto> {
    epoch_id:             u64,
    round:                u64,
    state_machine:        SMR,
    address:              Address,
    proposals:            ProposalCollector<T>,
    votes:                VoteCollector,
    authority:            AuthorityManage,
    hash_with_epoch:      HashMap<Hash, T>,
    full_transcation:     Arc<Mutex<HashMap<Hash, bool>>>,
    check_epoch_rx:       UnboundedReceiver<bool>,
    is_leader:            bool,
    leader_address:       Address,
    last_commit_round:    Option<u64>,
    last_commit_proposal: Option<Hash>,
    epoch_start:          Instant,
    epoch_interval:       u64,

    function: Arc<F>,
    pin_txs:  PhantomData<S>,
    util:     C,

    #[cfg(feature = "test")]
    timestamp: Timestamp,
}

impl<T, S, F, C> State<T, S, F, C>
where
    T: Codec + 'static,
    S: Codec,
    F: Consensus<T, S> + 'static,
    C: Crypto,
{
    /// Create a new state struct.
    pub fn new(smr: SMR, addr: Address, interval: u64, consensus: Arc<F>, crypto: C) -> Self {
        let (_tx, rx) = unbounded();

        State {
            epoch_id:             INIT_EPOCH_ID,
            round:                INIT_ROUND,
            state_machine:        smr,
            address:              addr,
            proposals:            ProposalCollector::new(),
            votes:                VoteCollector::new(),
            authority:            AuthorityManage::new(),
            hash_with_epoch:      HashMap::new(),
            full_transcation:     Arc::new(Mutex::new(HashMap::new())),
            check_epoch_rx:       rx,
            is_leader:            false,
            leader_address:       Address::default(),
            last_commit_round:    None,
            last_commit_proposal: None,
            epoch_start:          Instant::now(),
            epoch_interval:       interval,

            function: consensus,
            pin_txs:  PhantomData,
            util:     crypto,

            #[cfg(feature = "test")]
            timestamp:                          Timestamp::new(),
        }
    }

    /// Run state module.
    pub async fn run(
        &mut self,
        mut rx: UnboundedReceiver<(Context, OverlordMsg<T>)>,
        mut event: Event,
    ) -> ConsensusResult<()> {
        info!("Overlord: state start running");
        loop {
            select! {
                raw = rx.next() => self.handle_msg(raw).await?,
                evt = event.next() => self.handle_event(evt).await?,
            }
        }
    }

    /// A function to handle message from the network. Public this in the crate to do unit tests.
    pub(crate) async fn handle_msg(
        &mut self,
        msg: Option<(Context, OverlordMsg<T>)>,
    ) -> ConsensusResult<()> {
        let msg = msg.ok_or_else(|| ConsensusError::Other("Message sender dropped".to_string()))?;
        let (ctx, raw) = (msg.0, msg.1);

        match raw {
            OverlordMsg::SignedProposal(sp) => {
                if let Err(e) = self.handle_signed_proposal(ctx.clone(), sp).await {
                    error!("Overlord: state handle signed proposal error {:?}", e);
                }
                Ok(())
            }

            OverlordMsg::AggregatedVote(av) => {
                if let Err(e) = self.handle_aggregated_vote(ctx.clone(), av).await {
                    error!("Overlord: state handle aggregated vote error {:?}", e);
                }
                Ok(())
            }

            OverlordMsg::SignedVote(sv) => {
                if let Err(e) = self.handle_signed_vote(ctx.clone(), sv).await {
                    error!("Overlord: state handle signed vote error {:?}", e);
                }
                Ok(())
            }

            OverlordMsg::RichStatus(rs) => self.goto_new_epoch(ctx.clone(), rs).await,

            // This is for unit tests.
            #[cfg(test)]
            OverlordMsg::Commit(_) => Ok(()),
        }
    }

    /// A function to handle event from the SMR. Public this function in the crate to do unit tests.
    pub(crate) async fn handle_event(&mut self, event: Option<SMREvent>) -> ConsensusResult<()> {
        match event.ok_or_else(|| ConsensusError::Other("Event sender dropped".to_string()))? {
            SMREvent::NewRoundInfo {
                round,
                lock_round,
                lock_proposal,
                ..
            } => {
                if let Err(e) = self
                    .handle_new_round(round, lock_round, lock_proposal)
                    .await
                {
                    error!("Overlord: state handle new round error {:?}", e);
                }
                Ok(())
            }

            SMREvent::PrevoteVote { epoch_hash, .. } => {
                error!("Overlord: {:?}", self.hash_with_epoch.get(&epoch_hash));

                if let Err(e) = self.handle_prevote_vote(epoch_hash).await {
                    error!("Overlord: state handle prevote vote error {:?}", e);
                }
                Ok(())
            }

            SMREvent::PrecommitVote { epoch_hash, .. } => {
                error!("Overlord: {:?}", self.hash_with_epoch.get(&epoch_hash));

                if let Err(e) = self.handle_precommit_vote(epoch_hash).await {
                    error!("Overlord: state handle precommit vote error {:?}", e);
                }
                Ok(())
            }

            SMREvent::Commit(hash) => self.handle_commit(hash).await,
            _ => unreachable!(),
        }
    }

    /// On receiving a rich status will call this method. This status can be either the return value
    /// of the `commit()` interface, or lastest status after the synchronization is completed send
    /// by the overlord handler.
    ///
    /// If the difference between the status epoch ID and current's over one, get the last authority
    /// list of the status epoch ID firstly. Then update the epoch ID, authority_list and the epoch
    /// interval. Since it is possible to have received and cached the current epoch's proposals,
    /// votes and quorum certificates before, these should be re-checked as goto new epoch. Finally,
    /// trigger SMR to goto new epoch.
    async fn goto_new_epoch(&mut self, ctx: Context, status: Status) -> ConsensusResult<()> {
        #[cfg(feature = "test")]
        self.timestamp.update(Step::Commit);

        self.epoch_start = Instant::now();
        let new_epoch_id = status.epoch_id;
        let get_last_flag = new_epoch_id != self.epoch_id + 1;

        info!("Overlord: state goto new epoch {}", new_epoch_id);

        // Update epoch ID and authority list.
        self.epoch_id = new_epoch_id;
        self.round = INIT_ROUND;
        self.epoch_start = Instant::now();
        let mut auth_list = status.authority_list;
        self.authority.update(&mut auth_list, true);

        // If the status' epoch ID is much higher than the current,
        if get_last_flag {
            let mut tmp = self
                .function
                .get_authority_list(ctx, new_epoch_id - 1)
                .await
                .map_err(|err| {
                    ConsensusError::Other(format!("get authority list error {:?}", err))
                })?;
            self.authority.set_last_list(&mut tmp);
        }

        if let Some(interval) = status.interval {
            self.epoch_interval = interval;
        }

        // Clear outdated proposals and votes.
        self.proposals.flush(new_epoch_id - 1);
        self.votes.flush(new_epoch_id - 1);

        // Re-check proposals that have been in the proposal collector, of the current epoch ID.
        if let Some(proposals) = self.proposals.get_epoch_proposals(self.epoch_id) {
            self.re_check_proposals(proposals)?;
        }

        // Re-check votes and quorum certificates in the vote collector, of the current epoch ID.
        if let Some((votes, qcs)) = self.votes.get_epoch_votes(new_epoch_id) {
            self.re_check_votes(votes)?;
            self.re_check_qcs(qcs)?;
        }

        self.state_machine.new_epoch(new_epoch_id)?;
        Ok(())
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
        info!("Overlord: state goto new round {}", round);
        self.round = round;
        self.is_leader = false;

        if lock_round.is_some().bitxor(lock_proposal.is_some()) {
            return Err(ConsensusError::ProposalErr(
                "Lock round is inconsistent with lock proposal".to_string(),
            ));
        }

        if lock_proposal.is_none() {
            // Clear full transcation signal.
            self.full_transcation = Arc::new(Mutex::new(HashMap::new()));
            // Clear hash with epoch.
            self.hash_with_epoch.clear();
        }

        // If self is not proposer, check whether it has received current signed proposal before. If
        // has, then handle it.
        if !self.is_proposer()? {
            if let Ok(signed_proposal) = self.proposals.get(self.epoch_id, self.round) {
                return self
                    .handle_signed_proposal(Context::new(), signed_proposal)
                    .await;
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
        let ctx = Context::new();
        let (epoch, hash, polc) = if lock_round.is_none() {
            let (new_epoch, new_hash) =
                self.function
                    .get_epoch(ctx.clone(), self.epoch_id)
                    .await
                    .map_err(|err| ConsensusError::Other(format!("get epoch error {:?}", err)))?;
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

        self.hash_with_epoch.insert(hash.clone(), epoch.clone());

        let proposal = Proposal {
            epoch_id:   self.epoch_id,
            round:      self.round,
            content:    epoch.clone(),
            epoch_hash: hash.clone(),
            lock:       polc,
            proposer:   self.address.clone(),
        };

        // **TODO: parallelism**
        self.broadcast(
            Context::new(),
            OverlordMsg::SignedProposal(self.sign_proposal(proposal)?),
        )
        .await;

        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::Proposal,
            source:       TriggerSource::State,
            hash:         hash.clone(),
            round:        lock_round,
            epoch_id:     self.epoch_id,
        })?;

        info!(
            "Overlord: state trigger SMR epoch ID {}, round {}, type {:?}",
            self.epoch_id,
            self.round,
            TriggerType::Proposal
        );

        self.check_epoch(ctx, hash, epoch).await;
        Ok(())
    }

    /// This function only handle signed proposals which epoch ID and round are equal to current.
    /// Others will be ignored or storaged in the proposal collector.
    async fn handle_signed_proposal(
        &mut self,
        ctx: Context,
        signed_proposal: SignedProposal<T>,
    ) -> ConsensusResult<()> {
        let epoch_id = signed_proposal.proposal.epoch_id;
        let round = signed_proposal.proposal.round;

        info!(
            "Overlod: state receive a signed proposal epoch ID {}, round {}, from {:?}",
            epoch_id, round, signed_proposal.proposal.epoch_hash
        );

        // If the proposal epoch ID is lower than the current epoch ID - 1, or the proposal epoch ID
        // is equal to the current epoch ID and the proposal round is lower than the current round,
        // ignore it directly.
        if epoch_id < self.epoch_id - 1 || (epoch_id == self.epoch_id && round < self.round) {
            debug!("Overlord: state receive an outdated signed proposal");
            return Ok(());
        }

        // If the proposal epoch ID is higher than the current epoch ID or proposal epoch ID is
        // equal to the current epoch ID and the proposal round is higher than the current round,
        // cache it until that epoch ID.
        if epoch_id > self.epoch_id || (epoch_id == self.epoch_id && round > self.round) {
            self.proposals.insert(epoch_id, round, signed_proposal)?;
            debug!("Overlord: state receive a future signed proposal");
            return Ok(());
        }

        //  Verify proposal signature.
        let proposal = signed_proposal.proposal.clone();
        let signature = signed_proposal.signature.clone();
        self.verify_proposer(
            epoch_id,
            round,
            &proposal.proposer,
            epoch_id == self.epoch_id,
        )?;
        self.verify_signature(
            self.util.hash(Bytes::from(encode(&proposal))),
            signature,
            &proposal.proposer,
            MsgType::SignedProposal,
        )?;

        // Deal with proposal's epoch ID is equal to the current epoch ID - 1 and round is higher
        // than the last commit round. Retransmit prevote vote to the last commit proposal.
        if epoch_id == self.epoch_id - 1 {
            if let Some((last_round, last_proposal)) = self.last_commit_msg()? {
                if round <= last_round {
                    debug!("Overlord: state receive an outdated signed proposal");
                    return Ok(());
                }

                self.retransmit_vote(
                    ctx.clone(),
                    last_round,
                    last_proposal,
                    VoteType::Prevote,
                    proposal.proposer,
                )
                .await?;
            }
            return Ok(());
        }

        // If the signed proposal is with a lock, check the lock round and the QC then trigger it to
        // SMR. Otherwise, touch off SMR directly.
        let lock_round = if let Some(polc) = proposal.lock.clone() {
            debug!("Overlord: state receive a signed proposal with a lock");

            if !self.authority.is_above_threshold(
                polc.lock_votes.signature.address_bitmap.clone(),
                proposal.epoch_id == self.epoch_id,
            )? {
                return Err(ConsensusError::AggregatedSignatureErr(format!(
                    "aggregate signature below two thrids, proposal of epoch ID {:?}, round {:?}",
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

        info!(
            "Overlord: state trigger SMR proposal epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        let hash = proposal.epoch_hash.clone();
        let epoch = proposal.content.clone();

        self.hash_with_epoch.insert(hash.clone(), proposal.content);
        self.proposals.insert(self.epoch_id, self.round, signed_proposal)?;

        error!("Overlord: state {:?}", self.hash_with_epoch);

        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::Proposal,
            source:       TriggerSource::State,
            hash:         hash.clone(),
            round:        lock_round,
            epoch_id:     self.epoch_id,
        })?;

        info!(
            "Overlord: state tigger SMR epoch ID {}, round {}, type {:?}",
            self.epoch_id,
            self.round,
            TriggerType::Proposal
        );

        debug!("Overlord: state check the whole epoch");

        self.check_epoch(ctx, hash, epoch).await;
        Ok(())
    }

    async fn handle_prevote_vote(&mut self, hash: Hash) -> ConsensusResult<()> {
        #[cfg(feature = "test")]
        {
            // This is for test.
            self.timestamp.update(Step::Propose);
        }

        info!(
            "Overlord: state receive prevote vote event epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        let prevote = Vote {
            epoch_id:   self.epoch_id,
            round:      self.round,
            vote_type:  VoteType::Prevote,
            epoch_hash: hash,
            voter:      self.address.clone(),
        };

        let signed_vote = self.sign_vote(prevote)?;
        // **TODO: write Wal**
        if self.is_leader {
            self.votes
                .insert_vote(signed_vote.get_hash(), signed_vote, self.address.clone());
        } else {
            self.transmit(Context::new(), OverlordMsg::SignedVote(signed_vote))
                .await;
        }

        self.vote_process(VoteType::Prevote).await?;
        Ok(())
    }

    async fn handle_precommit_vote(&mut self, hash: Hash) -> ConsensusResult<()> {
        #[cfg(feature = "test")]
        {
            // This is for test.
            self.timestamp.update(Step::Prevote);
        }

        info!(
            "Overlord: state received precommit vote event epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        let precommit = Vote {
            epoch_id:   self.epoch_id,
            round:      self.round,
            vote_type:  VoteType::Precommit,
            epoch_hash: hash,
            voter:      self.address.clone(),
        };

        let signed_vote = self.sign_vote(precommit)?;
        // **TODO: write Wal**
        if self.is_leader {
            self.votes
                .insert_vote(signed_vote.get_hash(), signed_vote, self.address.clone());
        } else {
            self.transmit(Context::new(), OverlordMsg::SignedVote(signed_vote))
                .await;
        }

        self.vote_process(VoteType::Precommit).await?;
        Ok(())
    }

    async fn handle_commit(&mut self, hash: Hash) -> ConsensusResult<()> {
        #[cfg(feature = "test")]
        {
            // This is for test.
            self.timestamp.update(Step::Precommit);

            info!(
                "{:?}",
                json!({
                    "epoch_id": self.epoch_id,
                    "consensus_round": self.round,
                })
            );
        }

        info!(
            "Overlord: state receive commit event epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        debug!("Overlord: state get origin epoch");
        let epoch = self.epoch_id;

        let content = if let Some(tmp) = self.hash_with_epoch.get(&hash) {
            tmp.to_owned()
        } else {
            let proposal_vec = self.proposals.get_epoch_proposals(epoch).ok_or_else(|| {
                ConsensusError::StorageErr(format!("No proposal in epoch ID {}", epoch))
            })?;
            let mut res = u64::max_value();
            for (index, proposal) in proposal_vec.iter().enumerate() {
                if proposal.proposal.epoch_hash == hash {
                    res = index as u64;
                    break;
                }
            }

            if res == u64::max_value() {
                return Err(ConsensusError::Other(format!(
                    "Lose whole epoch epoch ID {}, round {}",
                    self.epoch_id, self.round
                )));
            }
            proposal_vec[res as usize].proposal.content.clone()
        };

        // let content = self
        //     .hash_with_epoch
        //     .get(&hash)
        //     .ok_or_else(|| {
        //         ConsensusError::Other(format!(
        //             "Lose whole epoch epoch ID {}, round {}",
        //             self.epoch_id, self.round
        //         ))
        //     })?
        //     .to_owned();

        debug!("Overlord: state generate proof");
        let qc = self
            .votes
            .get_qc(epoch, self.round, VoteType::Precommit)?
            .signature;

        let proof = Proof {
            epoch_id:   epoch,
            round:      self.round,
            epoch_hash: hash.clone(),
            signature:  qc,
        };
        let commit = Commit {
            epoch_id: epoch,
            content,
            proof,
        };

        self.last_commit_round = Some(self.round);
        self.last_commit_proposal = Some(hash);
        // **TODO: write Wal**

        // log consensus cost
        #[cfg(feature = "test")]
        {
            metrics!("consensus_cost" => self.epoch_start, "epoch ID": epoch);
        }

        let ctx = Context::new();
        let status = self
            .function
            .commit(ctx.clone(), epoch, commit)
            .await
            .map_err(|err| ConsensusError::Other(format!("commit error {:?}", err)))?;

        info!(
            "Overlord: achieve consensus in epoch ID {} costs {} round",
            self.epoch_id, self.round
        );

        if Instant::now() < self.epoch_start + Duration::from_millis(self.epoch_interval) {
            Delay::new_at(self.epoch_start + Duration::from_millis(self.epoch_interval))
                .await
                .map_err(|err| ConsensusError::Other(format!("Overlord delay error {:?}", err)))?;
        }

        self.goto_new_epoch(ctx, status).await?;
        Ok(())
    }

    /// The main process of handle signed vote is that only handle those epoch ID and round are both
    /// equal to the current. The lower votes will be ignored directly even if the epoch ID is equal
    /// to the `current epoch ID - 1` and the round is higher than the current round. The reason is
    /// that the effective leader must in the lower epoch, and the task of handling signed votes
    /// will be done by the leader. For the higher votes, check the signature and save them in
    /// the vote collector. Whenevet the current vote is received, a statistic is made to check
    /// if the sum of the voting weights corresponding to the hash exceeds the threshold.
    async fn handle_signed_vote(
        &mut self,
        ctx: Context,
        signed_vote: SignedVote,
    ) -> ConsensusResult<()> {
        let epoch_id = signed_vote.get_epoch();
        let round = signed_vote.get_round();
        let vote_type = if signed_vote.is_prevote() {
            VoteType::Prevote
        } else {
            VoteType::Precommit
        };

        info!(
            "Overlord: state receive a signed {:?} vote epoch ID {}, round {}, from {:?}",
            vote_type, epoch_id, round, signed_vote.vote.epoch_hash,
        );

        // If the vote epoch ID is lower than the current epoch ID - 1, or the vote epoch ID
        // is equal to the current epoch ID and the vote round is lower than the current round,
        // ignore it directly.
        if epoch_id < self.epoch_id || (epoch_id == self.epoch_id && round < self.round) {
            debug!("Overlord: state receive an outdated signed vote");
            return Ok(());
        }

        // All the votes must pass the verification of signature and address before be saved into
        // vote collector.
        let signature = signed_vote.signature.clone();
        let vote = signed_vote.vote.clone();
        self.verify_signature(
            self.util.hash(Bytes::from(encode(&vote))),
            signature,
            &vote.voter,
            MsgType::SignedVote,
        )?;
        self.verify_address(&vote.voter, true)?;
        self.votes
            .insert_vote(signed_vote.get_hash(), signed_vote, vote.voter);

        // If the vote epoch ID is higher than the current epoch ID, cache it and rehandle it by
        // entering the epoch. Else if the vote epoch ID is equal to the current epoch ID
        // and the vote round is higher than the current round, cache it until that round
        // and precess it.
        if epoch_id > self.epoch_id || (epoch_id == self.epoch_id && round > self.round) {
            debug!("Overlord: state receive a future signed vote");
            return Ok(());
        }

        if !self.is_leader {
            error!("Overlord: state is not leader but receive signed vote");
            return Ok(());
        }

        // Check whether there is a hash that vote weight is above the threshold. If no hash
        // achieved this, return directly.
        let epoch_hash = self.counting_vote(vote_type.clone())?;

        if epoch_hash.is_none() {
            debug!("Overlord: state counting of vote and no one above threshold");
            return Ok(());
        }

        info!(
            "Overlord: state counting a epoch hash that votes above threshold, epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        // Build the quorum certificate needs to aggregate signatures into an aggregate
        // signature besides the address bitmap.
        let mut epoch_hash = epoch_hash.unwrap();
        let qc = self.generate_qc(epoch_hash.clone(), vote_type.clone())?;

        debug!(
            "Overlord: state set QC epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        self.votes.set_qc(qc.clone());
        self.broadcast(ctx, OverlordMsg::AggregatedVote(qc)).await;

        if vote_type == VoteType::Prevote && !epoch_hash.is_empty() {
            epoch_hash = self.check_full_txs(epoch_hash).await?;
        }

        info!(
            "Overlord: state trigger SMR {:?} QC epoch ID {}, round {}",
            vote_type, self.epoch_id, self.round
        );
        self.state_machine.trigger(SMRTrigger {
            trigger_type: vote_type.clone().into(),
            source:       TriggerSource::State,
            hash:         epoch_hash,
            round:        Some(round),
            epoch_id:     self.epoch_id,
        })?;

        // This is for test
        info!(
            "Overlord: state trigger SMR epoch ID {}, round {}, type {:?}",
            self.epoch_id, self.round, vote_type,
        );

        Ok(())
    }

    /// The main process to handle aggregate votes contains four cases.
    ///
    /// 1. The QC is later than current which means the QC's epoch ID is higher than current or is
    /// equal to the current and the round is higher than current. In this cases, check the
    /// aggregate signature subject to availability, and save it.
    ///
    /// 2. The QC is equal to the current epoch ID and round. In this case, check the aggregate
    /// signature, then save it, and touch off SMR trigger.
    ///
    /// 3. The QC is equal to the `current epoch ID - 1` and the round is higher than the last
    /// commit round. In this case, check the aggregate signature firstly. If the type of the QC
    /// is precommit, ignore it. Otherwise, retransmit precommit vote to the last commit proposal.
    ///
    /// 4. Other cases, return `Ok(())` directly.
    async fn handle_aggregated_vote(
        &mut self,
        ctx: Context,
        aggregated_vote: AggregatedVote,
    ) -> ConsensusResult<()> {
        let epoch_id = aggregated_vote.get_epoch();
        let round = aggregated_vote.get_round();
        let qc_type = if aggregated_vote.is_prevote_qc() {
            VoteType::Prevote
        } else {
            VoteType::Precommit
        };

        info!(
            "Overlord: state receive an {:?} QC epoch {}, round {}, from {:?}",
            qc_type, epoch_id, round, aggregated_vote.epoch_hash
        );

        // If the vote epoch ID is lower than the current epoch ID - 1, or the vote epoch ID
        // is equal to the current epoch ID and the vote round is lower than the current round,
        // ignore it directly.
        if epoch_id < self.epoch_id - 1 || (epoch_id == self.epoch_id && round < self.round) {
            debug!("Overlord: state receive an outdated QC");
            return Ok(());
        } else if epoch_id > self.epoch_id {
            debug!("Overlord: state receive a future QC");
            self.votes.set_qc(aggregated_vote);
            return Ok(());
        }

        // Verify aggregate signature and check the sum of the voting weights corresponding to the
        // hash exceeds the threshold.
        self.verify_aggregated_signature(
            aggregated_vote.signature.clone(),
            epoch_id,
            qc_type.clone(),
        )?;

        if epoch_id == self.epoch_id && round > self.round {
            debug!("Overlord: state receive a future QC");
            self.votes.set_qc(aggregated_vote);
            return Ok(());
        }

        // Deal with QC's epoch ID is equal to the current epoch ID - 1 and round is higher than the
        // last commit round. If the QC is a prevoteQC, ignore it. Retransmit precommit vote to the
        // last commit proposal.
        if epoch_id == self.epoch_id - 1 {
            if let Some((last_round, last_proposal)) = self.last_commit_msg()? {
                if round <= last_round || qc_type == VoteType::Precommit {
                    debug!("Overlord: state receive an outdated QC");
                    return Ok(());
                }

                self.retransmit_vote(
                    ctx,
                    last_round,
                    last_proposal,
                    VoteType::Precommit,
                    aggregated_vote.leader,
                )
                .await?;
                return Ok(());
            } else {
                return Ok(());
            }
        }

        let qc_hash = aggregated_vote.epoch_hash.clone();
        self.votes.set_qc(aggregated_vote);

        debug!("Overlord: state check if get full transcations");

        let epoch_hash = if qc_type == VoteType::Prevote && !qc_hash.is_empty() {
            self.check_full_txs(qc_hash.clone()).await?
        } else {
            qc_hash
        };

        info!(
            "Overlord: state trigger SMR {:?} QC epoch ID {}, round {}",
            qc_type, self.epoch_id, self.round
        );

        self.state_machine.trigger(SMRTrigger {
            trigger_type: qc_type.clone().into(),
            source:       TriggerSource::State,
            hash:         epoch_hash,
            round:        Some(round),
            epoch_id:     self.epoch_id,
        })?;

        // This is for test
        info!(
            "Overlord: state trigger SMR epoch ID {}, round {}, type {:?}",
            self.epoch_id, self.round, qc_type,
        );

        Ok(())
    }

    /// On handling the signed vote, some signed votes and quorum certificates might have
    /// been cached in the vote collector. So it should check whether there is votes or quorum
    /// certificates exsits or not. If self node is not the leader, check if there is prevoteQC
    /// exits. If self node is the leader, check if there is signed prevote vote exsits. It
    /// should be noted that when self is the leader, and the vote type is prevote, the process
    /// should be the same as the handle signed vote.
    async fn vote_process(&mut self, vote_type: VoteType) -> ConsensusResult<()> {
        if !self.is_leader {
            if let Ok(qc) = self
                .votes
                .get_qc(self.epoch_id, self.round, vote_type.clone())
            {
                let mut epoch_hash = qc.epoch_hash.clone();
                if vote_type == VoteType::Prevote && !epoch_hash.is_empty() {
                    epoch_hash = self.check_full_txs(epoch_hash).await?;
                }

                self.state_machine.trigger(SMRTrigger {
                    trigger_type: qc.vote_type.clone().into(),
                    source:       TriggerSource::State,
                    hash:         epoch_hash,
                    round:        Some(self.round),
                    epoch_id:     self.epoch_id,
                })?;

                info!(
                    "Overlord: state trigger SMR epoch ID {}, round {}, type {:?}",
                    self.epoch_id, self.round, qc.vote_type,
                );

                return Ok(());
            }
        } else if let Some(mut epoch_hash) = self.counting_vote(vote_type.clone())? {
            let qc = self.generate_qc(epoch_hash.clone(), vote_type.clone())?;
            self.votes.set_qc(qc.clone());
            self.broadcast(Context::new(), OverlordMsg::AggregatedVote(qc))
                .await;

            if vote_type == VoteType::Prevote && !epoch_hash.is_empty() {
                epoch_hash = self.check_full_txs(epoch_hash).await?;
            }

            info!(
                "Overlord: state trigger SMR {:?} QC epoch ID {}, round {}",
                vote_type, self.epoch_id, self.round
            );

            self.state_machine.trigger(SMRTrigger {
                trigger_type: vote_type.clone().into(),
                source:       TriggerSource::State,
                hash:         epoch_hash,
                round:        Some(self.round),
                epoch_id:     self.epoch_id,
            })?;

            // This is for test
            info!(
                "Overlord: state trigger SMR epoch ID {}, round {}, type {:?}",
                self.epoch_id, self.round, vote_type,
            );
        }
        Ok(())
    }

    fn counting_vote(&mut self, vote_type: VoteType) -> ConsensusResult<Option<Hash>> {
        let vote_map = self
            .votes
            .get_vote_map(self.epoch_id, self.round, vote_type.clone())?;
        let threshold = self.authority.get_vote_weight_sum(true)? * 2;

        for (hash, set) in vote_map.iter() {
            let mut acc = 0u8;
            for addr in set.iter() {
                acc += self.authority.get_vote_weight(addr)?;
            }
            if u64::from(acc) * 3 > threshold {
                return Ok(Some(hash.to_owned()));
            }
        }
        Ok(None)
    }

    fn generate_qc(
        &mut self,
        epoch_hash: Hash,
        vote_type: VoteType,
    ) -> ConsensusResult<AggregatedVote> {
        let votes =
            self.votes
                .get_votes(self.epoch_id, self.round, vote_type.clone(), &epoch_hash)?;

        debug!("Overlord: state build aggregated signature");

        let len = votes.len();
        let mut signatures = Vec::with_capacity(len);
        let mut voters = Vec::with_capacity(len);
        for vote in votes.into_iter() {
            signatures.push(vote.signature);
            voters.push(vote.vote.voter);
        }

        let set = voters.iter().cloned().collect::<HashSet<_>>();
        let mut bit_map = BitVec::from_elem(self.authority.current_len(), false);
        for (index, addr) in self.authority.get_addres_ref().iter().enumerate() {
            if set.contains(addr) {
                bit_map.set(index, true);
            }
        }

        let aggregated_signature = AggregatedSignature {
            signature:      self.aggregate_signatures(signatures, voters)?,
            address_bitmap: Bytes::from(bit_map.to_bytes()),
        };
        let qc = AggregatedVote {
            signature: aggregated_signature,
            vote_type,
            epoch_id: self.epoch_id,
            round: self.round,
            epoch_hash,
            leader: self.address.clone(),
        };
        Ok(qc)
    }

    fn re_check_proposals(&mut self, proposals: Vec<SignedProposal<T>>) -> ConsensusResult<()> {
        debug!("Overlord: state re-check future signed proposals");
        for sp in proposals.into_iter() {
            let signature = sp.signature.clone();
            let proposal = sp.proposal.clone();

            if self
                .verify_proposer(proposal.epoch_id, proposal.round, &proposal.proposer, true)
                .is_ok()
                && self
                    .verify_signature(
                        self.util.hash(Bytes::from(encode(&proposal))),
                        signature,
                        &proposal.proposer,
                        MsgType::SignedProposal,
                    )
                    .is_ok()
            {
                self.proposals
                    .insert(proposal.epoch_id, proposal.round, sp)?;
            }
        }
        Ok(())
    }

    fn re_check_votes(&mut self, votes: Vec<SignedVote>) -> ConsensusResult<()> {
        debug!("Overlord: state re-check future signed votes");
        if votes.is_empty() {
            return Ok(());
        }

        for sv in votes.into_iter() {
            let signature = sv.signature.clone();
            let vote = sv.vote.clone();

            if self
                .verify_signature(
                    self.util.hash(Bytes::from(encode(&vote))),
                    signature,
                    &vote.voter,
                    MsgType::SignedVote,
                )
                .is_ok()
                && self.verify_address(&vote.voter, true).is_ok()
            {
                self.votes.insert_vote(sv.get_hash(), sv, vote.voter);
            }
        }
        Ok(())
    }

    fn re_check_qcs(&mut self, qcs: Vec<AggregatedVote>) -> ConsensusResult<()> {
        debug!("Overlord: state re-check future QCs");
        if qcs.is_empty() {
            return Ok(());
        }

        for qc in qcs.into_iter() {
            if self
                .verify_aggregated_signature(
                    qc.signature.clone(),
                    self.epoch_id,
                    qc.vote_type.clone(),
                )
                .is_ok()
            {
                self.votes.set_qc(qc);
            }
        }
        Ok(())
    }

    /// Check last commit round and last commit proposal.
    fn last_commit_msg(&self) -> ConsensusResult<Option<(u64, Hash)>> {
        debug!("Overlord: state check last commit message");
        if self
            .last_commit_round
            .is_some()
            .bitxor(self.last_commit_proposal.is_some())
        {
            return Err(ConsensusError::Other(
                "Last commit things conflict".to_string(),
            ));
        }

        if self.last_commit_round.is_none() {
            return Ok(None);
        }
        let last_round = self.last_commit_round.clone().unwrap();
        let last_proposal = self.last_commit_proposal.clone().unwrap();
        Ok(Some((last_round, last_proposal)))
    }

    /// If self is not the proposer of the epoch ID and round, set leader address as the proposer
    /// address.
    fn is_proposer(&mut self) -> ConsensusResult<bool> {
        let proposer = self
            .authority
            .get_proposer(self.epoch_id + self.round, true)?;

        if proposer == self.address {
            return Ok(true);
        }
        // If self is not the proposer, set the leader address to the proposer address.
        self.leader_address = proposer;
        Ok(false)
    }

    fn sign_proposal(&self, proposal: Proposal<T>) -> ConsensusResult<SignedProposal<T>> {
        debug!("Overlord: state sign a proposal");
        let signature = self
            .util
            .sign(self.util.hash(Bytes::from(encode(&proposal))))
            .map_err(|err| ConsensusError::CryptoErr(format!("{:?}", err)))?;

        Ok(SignedProposal {
            signature,
            proposal,
        })
    }

    fn sign_vote(&self, vote: Vote) -> ConsensusResult<SignedVote> {
        debug!("Overlord: state sign a vote");
        let signature = self
            .util
            .sign(self.util.hash(Bytes::from(encode(&vote))))
            .map_err(|err| ConsensusError::CryptoErr(format!("{:?}", err)))?;
        Ok(SignedVote { signature, vote })
    }

    fn aggregate_signatures(
        &self,
        signatures: Vec<Signature>,
        voters: Vec<Address>,
    ) -> ConsensusResult<Signature> {
        debug!("Overlord: state aggregate signatures");
        let signature = self
            .util
            .aggregate_signatures(signatures, voters)
            .map_err(|err| ConsensusError::CryptoErr(format!("{:?}", err)))?;
        Ok(signature)
    }

    fn verify_signature(
        &self,
        hash: Hash,
        signature: Signature,
        address: &Address,
        msg_type: MsgType,
    ) -> ConsensusResult<()> {
        debug!("Overlord: state verify a signature");
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

    fn verify_aggregated_signature(
        &self,
        signature: AggregatedSignature,
        epoch: u64,
        vote_type: VoteType,
    ) -> ConsensusResult<()> {
        debug!("Overlord: state verify an aggregated signature");
        if !self
            .authority
            .is_above_threshold(signature.address_bitmap.clone(), epoch == self.epoch_id)?
        {
            return Err(ConsensusError::AggregatedSignatureErr(format!(
                "{:?} QC of epoch {}, round {} is not above threshold",
                vote_type.clone(),
                self.epoch_id,
                self.round
            )));
        }

        self.util
            .verify_aggregated_signature(signature)
            .map_err(|err| {
                ConsensusError::AggregatedSignatureErr(format!(
                    "{:?} aggregate signature error {:?}",
                    vote_type, err
                ))
            })?;
        Ok(())
    }

    fn verify_proposer(
        &self,
        epoch_id: u64,
        round: u64,
        address: &Address,
        is_current: bool,
    ) -> ConsensusResult<()> {
        debug!("Overlord: state verify a proposer");
        self.verify_address(address, is_current)?;
        if address != &self.authority.get_proposer(epoch_id + round, is_current)? {
            return Err(ConsensusError::ProposalErr("Invalid proposer".to_string()));
        }
        Ok(())
    }

    /// Check whether the given address is included in the corresponding authority list.
    fn verify_address(&self, address: &Address, is_current: bool) -> ConsensusResult<()> {
        if !self.authority.contains(address, is_current)? {
            return Err(ConsensusError::InvalidAddress);
        }
        Ok(())
    }

    async fn transmit(&self, ctx: Context, msg: OverlordMsg<T>) {
        info!(
            "Overlord: state transmit a message to leader epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        let _ = self
            .function
            .transmit_to_relayer(ctx, self.leader_address.clone(), msg)
            .await
            .map_err(|err| {
                error!(
                    "Overlord: state transmit message to leader failed {:?}",
                    err
                );
            });
    }

    async fn retransmit_vote(
        &self,
        ctx: Context,
        last_round: u64,
        hash: Hash,
        v_type: VoteType,
        leader_address: Address,
    ) -> ConsensusResult<()> {
        let vote = Vote {
            epoch_id:   self.epoch_id - 1,
            round:      last_round,
            epoch_hash: hash,
            voter:      self.address.clone(),
            vote_type:  v_type,
        };

        debug!("Overlord: state re-transmit last epoch vote");

        let _ = self
            .function
            .transmit_to_relayer(
                ctx,
                leader_address,
                OverlordMsg::SignedVote(self.sign_vote(vote)?),
            )
            .await
            .map_err(|err| {
                error!(
                    "Overlord: state transmit message to leader failed {:?}",
                    err
                );
            });

        Ok(())
    }

    async fn broadcast(&self, ctx: Context, msg: OverlordMsg<T>) {
        info!(
            "Overlord: state broadcast a message to others epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        let _ = self
            .function
            .broadcast_to_other(ctx, msg)
            .await
            .map_err(|err| {
                error!(
                    "Overlord: state transmit message to leader failed {:?}",
                    err
                );
            });
    }

    async fn check_epoch(&mut self, ctx: Context, hash: Hash, epoch: T) {
        let epoch_id = self.epoch_id;
        let round = self.round;
        let tx_signal = Arc::clone(&self.full_transcation);
        let function = Arc::clone(&self.function);
        let (new_tx, new_rx) = unbounded();
        let mempool_tx = new_tx.clone();
        self.check_epoch_rx = new_rx;

        runtime::spawn(async move {
            if let Err(e) =
                check_current_epoch(ctx, function, tx_signal, epoch_id, hash, epoch).await
            {
                error!(
                    "Overlord: state check epoch failed, epoch ID {}, round {}, error {:?}",
                    epoch_id, round, e
                )
            }

            if let Err(e) = mempool_tx.unbounded_send(CHECK_EPOCH_FLAG) {
                error!(
                    "Overlord: state send check epoch flag failed, epoch ID {}, round {}, error {:?}",
                    epoch_id, round, e
                );
            }
        });

        runtime::spawn(async move {
            let _ = Delay::new(Duration::from_millis(1500)).await;
            if let Err(e) = new_tx.unbounded_send(false) {
                error!(
                    "Overlord: state send check epoch flag failed, epoch ID {}, round {}, error {:?}",
                    epoch_id, round, e
                );
            }
        });
    }

    async fn check_full_txs(&mut self, hash: Hash) -> ConsensusResult<Hash> {
        {
            let map = self.full_transcation.lock();
            if let Some(res) = map.get(&hash) {
                if *res {
                    return Ok(hash.clone());
                } else {
                    return Ok(Hash::new());
                }
            }
        }

        let flag =
            self.check_epoch_rx.next().await.ok_or_else(|| {
                ConsensusError::ChannelErr("receive check txs result".to_string())
            })?;

        let epoch_hash = {
            let mut map = self.full_transcation.lock();
            map.insert(hash.clone(), flag);

            if flag {
                hash
            } else {
                warn!(
                    "Overlord: state check that does not get full transitions when handle signed vote"
                );
                Hash::new()
            }
        };
        Ok(epoch_hash)
    }

    #[cfg(test)]
    pub fn set_condition(&mut self, epoch_id: u64, round: u64) {
        self.epoch_id = epoch_id;
        self.round = round;
    }

    // #[cfg(test)]
    // pub fn set_authority(&mut self, mut authority: Vec<Node>) {
    //     self.authority.update(&mut authority, false);
    // }

    #[cfg(test)]
    pub fn set_proposal_collector(&mut self, collector: ProposalCollector<T>) {
        self.proposals = collector;
    }

    #[cfg(test)]
    pub fn set_vote_collector(&mut self, collector: VoteCollector) {
        self.votes = collector;
    }

    #[cfg(test)]
    pub fn set_full_transaction(&mut self, hash: Hash) {
        let mut map = self.full_transcation.lock();
        map.insert(hash, true);
    }

    #[cfg(test)]
    pub fn set_hash_with_epoch(&mut self, hash_with_epoch: HashMap<Hash, T>) {
        self.hash_with_epoch = hash_with_epoch;
    }
}

async fn check_current_epoch<U: Consensus<T, S>, T: Codec, S: Codec>(
    ctx: Context,
    function: Arc<U>,
    tx_signal: Arc<Mutex<HashMap<Hash, bool>>>,
    epoch_id: u64,
    hash: Hash,
    epoch: T,
) -> ConsensusResult<()> {
    let transcations = function
        .check_epoch(ctx, epoch_id, hash.clone(), epoch)
        .await;

    let res = transcations.is_ok();
    let mut map = tx_signal.lock();
    map.insert(hash, res);
    // TODO: write Wal
    Ok(())
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use log::info;
    use serde_json::json;

    #[test]
    fn test_json() {
        let tmp = Duration::from_millis(200);
        let cost = tmp.as_millis() as u64;

        info!(
            "{:?}",
            json!({
                "epoch_id": 1u64,
                "consensus_round": 1u64,
                "consume": cost,
            })
        );
    }
}
