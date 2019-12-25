use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::string::ToString;
use std::time::{Duration, Instant};
use std::{ops::BitXor, sync::Arc};

use bit_vec::BitVec;
use bytes::Bytes;
use creep::Context;
use derive_more::Display;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::{select, StreamExt};
use futures_timer::Delay;
use log::{debug, error, info, warn};
use moodyblues_sdk::trace;
use rlp::encode;
use serde_json::json;

use crate::smr::smr_types::{SMREvent, SMRTrigger, TriggerSource, TriggerType};
use crate::smr::{Event, SMRHandler};
use crate::state::collection::{ProposalCollector, VoteCollector};
use crate::types::{
    Address, AggregatedSignature, AggregatedVote, Commit, Hash, OverlordMsg, PoLC, Proof, Proposal,
    Signature, SignedProposal, SignedVote, Status, VerifyResp, Vote, VoteType,
};
use crate::{error::ConsensusError, utils::auth_manage::AuthorityManage};
use crate::{Codec, Consensus, ConsensusResult, Crypto, INIT_EPOCH_ID, INIT_ROUND};

const FUTURE_EPOCH_GAP: u64 = 5;
const FUTURE_ROUND_GAP: u64 = 10;

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
    epoch_id:            u64,
    round:               u64,
    state_machine:       SMRHandler,
    address:             Address,
    proposals:           ProposalCollector<T>,
    votes:               VoteCollector,
    authority:           AuthorityManage,
    hash_with_epoch:     HashMap<Hash, T>,
    is_full_transcation: HashMap<Hash, bool>,
    is_leader:           bool,
    leader_address:      Address,
    last_commit_qc:      Option<AggregatedVote>,
    epoch_start:         Instant,
    epoch_interval:      u64,

    resp_tx:  UnboundedSender<VerifyResp<S>>,
    function: Arc<F>,
    util:     C,
}

impl<T, S, F, C> State<T, S, F, C>
where
    T: Codec + 'static,
    S: Codec + 'static,
    F: Consensus<T, S> + 'static,
    C: Crypto,
{
    /// Create a new state struct.
    pub fn new(
        smr: SMRHandler,
        addr: Address,
        interval: u64,
        consensus: Arc<F>,
        crypto: C,
    ) -> (Self, UnboundedReceiver<VerifyResp<S>>) {
        let (tx, rx) = unbounded();

        let state = State {
            epoch_id:            INIT_EPOCH_ID,
            round:               INIT_ROUND,
            state_machine:       smr,
            address:             addr,
            proposals:           ProposalCollector::new(),
            votes:               VoteCollector::new(),
            authority:           AuthorityManage::new(),
            hash_with_epoch:     HashMap::new(),
            is_full_transcation: HashMap::new(),
            is_leader:           false,
            leader_address:      Address::default(),
            last_commit_qc:      None,
            epoch_start:         Instant::now(),
            epoch_interval:      interval,

            resp_tx:  tx,
            function: consensus,
            util:     crypto,
        };
        (state, rx)
    }

    /// Run state module.
    pub async fn run(
        &mut self,
        mut raw_rx: UnboundedReceiver<(Context, OverlordMsg<T>)>,
        mut event: Event,
        mut verify_resp: UnboundedReceiver<VerifyResp<S>>,
    ) -> ConsensusResult<()> {
        info!("Overlord: state start running");

        loop {
            select! {
                raw = raw_rx.next() => self.handle_msg(raw).await?,
                evt = event.next() => self.handle_event(evt).await?,
                res = verify_resp.next() => self.handle_resp(res)?,
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
                    trace::error(
                        "handle_signed_proposal".to_string(),
                        Some(json!({
                            "epoch_id": self.epoch_id,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle signed proposal error {:?}", e);
                }
                Ok(())
            }

            OverlordMsg::AggregatedVote(av) => {
                if let Err(e) = self.handle_aggregated_vote(ctx.clone(), av).await {
                    trace::error(
                        "handle_aggregated_vote".to_string(),
                        Some(json!({
                            "epoch_id": self.epoch_id,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle aggregated vote error {:?}", e);
                }
                Ok(())
            }

            OverlordMsg::SignedVote(sv) => {
                if let Err(e) = self.handle_signed_vote(ctx.clone(), sv).await {
                    trace::error(
                        "handle_signed_vote".to_string(),
                        Some(json!({
                            "epoch_id": self.epoch_id,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle signed vote error {:?}", e);
                }
                Ok(())
            }

            OverlordMsg::RichStatus(rs) => self.goto_new_epoch(ctx.clone(), rs, true).await,

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
                    trace::error(
                        "handle_new_round".to_string(),
                        Some(json!({
                            "epoch_id": self.epoch_id,
                            "round": round,
                            "is lock": lock_round.is_some(),
                        })),
                    );

                    error!("Overlord: state handle new round error {:?}", e);
                }
                Ok(())
            }

            SMREvent::PrevoteVote { epoch_hash, .. } => {
                if let Err(e) = self.handle_vote_event(epoch_hash, VoteType::Prevote).await {
                    trace::error(
                        "handle_prevote_vote".to_string(),
                        Some(json!({
                            "epoch_id": self.epoch_id,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle prevote vote error {:?}", e);
                }
                Ok(())
            }

            SMREvent::PrecommitVote { epoch_hash, .. } => {
                if let Err(e) = self
                    .handle_vote_event(epoch_hash, VoteType::Precommit)
                    .await
                {
                    trace::error(
                        "handle_precommit_vote".to_string(),
                        Some(json!({
                            "epoch_id": self.epoch_id,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle precommit vote error {:?}", e);
                }
                Ok(())
            }

            SMREvent::Commit(hash) => {
                if let Err(e) = self.handle_commit(hash).await {
                    trace::error(
                        "handle_commit_event".to_string(),
                        Some(json!({
                            "epoch_id": self.epoch_id,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle commit error {:?}", e);
                }
                Ok(())
            }

            _ => unreachable!(),
        }
    }

    fn handle_resp(&mut self, msg: Option<VerifyResp<S>>) -> ConsensusResult<()> {
        let resp = msg.ok_or_else(|| ConsensusError::Other("Event sender dropped".to_string()))?;
        if resp.epoch_id != self.epoch_id {
            return Ok(());
        }

        let epoch_hash = resp.epoch_hash.clone();
        info!(
            "Overlord: state receive verify response epoch ID {:?}, hash {:?}",
            resp.epoch_id, epoch_hash
        );

        self.is_full_transcation
            .insert(epoch_hash.clone(), resp.full_txs.is_some());

        if let Some(qc) =
            self.votes
                .get_qc_by_hash(self.epoch_id, epoch_hash.clone(), VoteType::Precommit)
        {
            self.state_machine.trigger(SMRTrigger {
                trigger_type: TriggerType::PrecommitQC,
                source:       TriggerSource::State,
                hash:         qc.epoch_hash,
                round:        Some(self.round),
                epoch_id:     self.epoch_id,
            })?;
        } else if let Some(qc) =
            self.votes
                .get_qc_by_hash(self.epoch_id, epoch_hash, VoteType::Prevote)
        {
            self.state_machine.trigger(SMRTrigger {
                trigger_type: TriggerType::PrevoteQC,
                source:       TriggerSource::State,
                hash:         qc.epoch_hash,
                round:        Some(self.round),
                epoch_id:     self.epoch_id,
            })?;
        }
        Ok(())
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
    async fn goto_new_epoch(
        &mut self,
        ctx: Context,
        status: Status,
        get_last_flag: bool,
    ) -> ConsensusResult<()> {
        let new_epoch_id = status.epoch_id;
        self.epoch_id = new_epoch_id;
        self.round = INIT_ROUND;
        info!("Overlord: state goto new epoch {}", self.epoch_id);

        trace::start_epoch(new_epoch_id);

        // Update epoch ID and authority list.
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
        self.hash_with_epoch.clear();

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
        trace::start_round(round, self.epoch_id);

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
        trace::start_step("become_leader".to_string(), self.round, self.epoch_id);
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
                .get_qc_by_id(self.epoch_id, round, VoteType::Prevote)
                .map_err(|err| ConsensusError::ProposalErr(format!("{:?} when propose", err)))?;
            let polc = PoLC {
                lock_round: round,
                lock_votes: qc,
            };
            (epoch.to_owned(), hash, Some(polc))
        };

        self.hash_with_epoch
            .entry(hash.clone())
            .or_insert_with(|| epoch.clone());

        let proposal = Proposal {
            epoch_id:   self.epoch_id,
            round:      self.round,
            content:    epoch.clone(),
            epoch_hash: hash.clone(),
            lock:       polc.clone(),
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
    /// Others will be ignored or stored in the proposal collector.
    async fn handle_signed_proposal(
        &mut self,
        ctx: Context,
        signed_proposal: SignedProposal<T>,
    ) -> ConsensusResult<()> {
        let epoch_id = signed_proposal.proposal.epoch_id;
        let round = signed_proposal.proposal.round;

        info!(
            "Overlord: state receive a signed proposal epoch ID {}, round {}",
            epoch_id, round,
        );

        if self.filter_signed_proposal(epoch_id, round, &signed_proposal)? {
            return Ok(());
        }

        trace::receive_proposal(
            "receive_signed_proposal".to_string(),
            epoch_id,
            round,
            hex::encode(signed_proposal.proposal.proposer.clone()),
            hex::encode(signed_proposal.proposal.epoch_hash.clone()),
            None,
        );

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
            self.retransmit_qc(ctx.clone(), proposal.proposer).await?;
            return Ok(());
        }

        // If the signed proposal is with a lock, check the lock round and the QC then trigger it to
        // SMR. Otherwise, touch off SMR directly.
        let lock_round = if let Some(polc) = proposal.lock.clone() {
            debug!("Overlord: state receive a signed proposal with a lock");

            if !self.authority.is_above_threshold(
                &polc.lock_votes.signature.address_bitmap,
                proposal.epoch_id == self.epoch_id,
            )? {
                return Err(ConsensusError::AggregatedSignatureErr(format!(
                    "aggregate signature below two thirds, proposal of epoch ID {:?}, round {:?}",
                    proposal.epoch_id, proposal.round
                )));
            }

            let voters = self.authority.get_voters(
                &polc.lock_votes.signature.address_bitmap,
                proposal.epoch_id == self.epoch_id,
            )?;
            self.util
                .verify_aggregated_signature(
                    polc.lock_votes.signature.signature,
                    proposal.epoch_hash.clone(),
                    voters,
                )
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
        self.proposals
            .insert(self.epoch_id, self.round, signed_proposal)?;

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

        debug!("Overlord: state check the whole epoch");
        self.check_epoch(ctx, hash, epoch).await;
        Ok(())
    }

    async fn handle_vote_event(&mut self, hash: Hash, vote_type: VoteType) -> ConsensusResult<()> {
        // If the vote epoch hash is empty, do nothing.
        if hash.is_empty() {
            return Ok(());
        }

        info!(
            "Overlord: state receive {:?} vote event epoch ID {}, round {}",
            vote_type.clone(),
            self.epoch_id,
            self.round
        );

        let tmp_type: String = vote_type.to_string();
        trace::custom(
            "receive_vote_event".to_string(),
            Some(json!({
                "epoch_id": self.epoch_id,
                "round": self.round,
                "vote type": tmp_type,
                "vote hash": hex::encode(hash.clone()),
            })),
        );

        let signed_vote = self.sign_vote(Vote {
            epoch_id:   self.epoch_id,
            round:      self.round,
            vote_type:  vote_type.clone(),
            epoch_hash: hash,
        })?;

        if self.is_leader {
            self.votes
                .insert_vote(signed_vote.get_hash(), signed_vote, self.address.clone());
        } else {
            self.transmit(Context::new(), OverlordMsg::SignedVote(signed_vote))
                .await;
        }

        self.vote_process(vote_type).await?;
        Ok(())
    }

    async fn handle_commit(&mut self, hash: Hash) -> ConsensusResult<()> {
        info!(
            "Overlord: state receive commit event epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        trace::custom(
            "receive_commit_event".to_string(),
            Some(json!({
                "epoch_id": self.epoch_id,
                "round": self.round,
                "epoch hash": hex::encode(hash.clone()),
            })),
        );

        debug!("Overlord: state get origin epoch");
        let epoch = self.epoch_id;
        let content = if let Some(tmp) = self.hash_with_epoch.get(&hash) {
            tmp.to_owned()
        } else {
            return Err(ConsensusError::Other(format!(
                "Lose whole epoch epoch ID {}, round {}",
                self.epoch_id, self.round
            )));
        };

        debug!("Overlord: state generate proof");
        let qc = self
            .votes
            .get_qc_by_hash(epoch, hash.clone(), VoteType::Precommit);
        if qc.is_none() {
            return Err(ConsensusError::StorageErr("Lose precommit QC".to_string()));
        }

        let qc = qc.unwrap();
        let proof = Proof {
            epoch_id:   epoch,
            round:      self.round,
            epoch_hash: hash.clone(),
            signature:  qc.signature.clone(),
        };
        let commit = Commit {
            epoch_id: epoch,
            content,
            proof,
        };

        self.last_commit_qc = Some(qc);
        // **TODO: write Wal**

        let ctx = Context::new();
        let status = self
            .function
            .commit(ctx.clone(), epoch, commit)
            .await
            .map_err(|err| ConsensusError::Other(format!("commit error {:?}", err)))?;

        info!(
            "Overlord: achieve consensus in epoch ID {} costs {} round",
            self.epoch_id,
            self.round + 1
        );

        let mut auth_list = status.authority_list.clone();
        self.authority.update(&mut auth_list, true);

        let cost = Instant::now() - self.epoch_start;
        if self.next_proposer(status.epoch_id)? && cost < Duration::from_millis(self.epoch_interval)
        {
            Delay::new(Duration::from_millis(self.epoch_interval) - cost).await;
        }

        self.goto_new_epoch(ctx, status, false).await?;
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
        if signed_vote.vote.epoch_hash.is_empty() {
            warn!("Overlord: state receive an empty signed vote");
            return Ok(());
        }

        let epoch_id = signed_vote.get_epoch();
        let round = signed_vote.get_round();
        let vote_type = if signed_vote.is_prevote() {
            VoteType::Prevote
        } else {
            VoteType::Precommit
        };

        info!(
            "Overlord: state receive a signed {:?} vote epoch ID {}, round {}",
            vote_type, epoch_id, round,
        );

        if epoch_id != self.epoch_id - 1
            && (!self.is_leader || epoch_id != self.epoch_id || round != self.round)
        {
            return Ok(());
        }

        let tmp_type: String = vote_type.to_string();
        trace::receive_vote(
            "receive_signed_vote".to_string(),
            epoch_id,
            round,
            hex::encode(signed_vote.voter.clone()),
            hex::encode(signed_vote.vote.epoch_hash.clone()),
            Some(json!({ "vote type": tmp_type })),
        );

        // All the votes must pass the verification of signature and address before be saved into
        // vote collector.
        let signature = signed_vote.signature.clone();
        let voter = signed_vote.voter.clone();
        let vote = signed_vote.vote.clone();
        self.verify_signature(
            self.util.hash(Bytes::from(encode(&vote))),
            signature,
            &voter,
            MsgType::SignedVote,
        )?;
        self.verify_address(&voter, true)?;

        if epoch_id == self.epoch_id - 1 {
            self.retransmit_qc(ctx, voter).await?;
            return Ok(());
        }

        // Check if the quorum certificate has generated before check whether there is a hash that
        // vote weight is above the threshold. If no hash achieved this, return directly.
        if self
            .votes
            .get_qc_by_id(epoch_id, round, vote_type.clone())
            .is_ok()
        {
            return Ok(());
        }

        self.votes
            .insert_vote(signed_vote.get_hash(), signed_vote, voter);
        let epoch_hash = self.counting_vote(vote_type.clone())?;

        if epoch_hash.is_none() {
            debug!("Overlord: state counting of vote and no one above threshold");
            return Ok(());
        }

        let mut epoch_hash = epoch_hash.unwrap();
        info!(
            "Overlord: state counting a epoch hash that votes above threshold, epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        // Build the quorum certificate needs to aggregate signatures into an aggregate
        // signature besides the address bitmap.
        let qc = self.generate_qc(epoch_hash.clone(), vote_type.clone())?;

        debug!(
            "Overlord: state set QC epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        self.votes.set_qc(qc.clone());
        self.broadcast(ctx, OverlordMsg::AggregatedVote(qc)).await;

        if vote_type == VoteType::Prevote {
            match self.check_full_txs(epoch_hash) {
                Some(tmp) => epoch_hash = tmp,
                None => return Ok(()),
            }
        } else if !self.try_get_full_txs(&epoch_hash) {
            return Ok(());
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
    /// is precommit, ignore it. Otherwise, retransmit precommit QC.
    ///
    /// 4. Other cases, return `Ok(())` directly.
    async fn handle_aggregated_vote(
        &mut self,
        _ctx: Context,
        aggregated_vote: AggregatedVote,
    ) -> ConsensusResult<()> {
        if aggregated_vote.epoch_hash.is_empty() {
            warn!("Overlord: state receive an empty aggregated vote");
            return Ok(());
        }

        let epoch_id = aggregated_vote.get_epoch();
        let round = aggregated_vote.get_round();
        let qc_type = if aggregated_vote.is_prevote_qc() {
            VoteType::Prevote
        } else {
            VoteType::Precommit
        };

        info!(
            "Overlord: state receive an {:?} QC epoch {}, round {}",
            qc_type, epoch_id, round,
        );

        // If the vote epoch ID is lower than the current epoch ID, ignore it directly. If the vote
        // epoch ID is higher than current epoch ID, save it and return Ok;
        match epoch_id.cmp(&self.epoch_id) {
            Ordering::Less => {
                debug!(
                    "Overlord: state receive an outdated QC, epoch ID {}, round {}",
                    epoch_id, round,
                );
                return Ok(());
            }

            Ordering::Greater => {
                if self.epoch_id + FUTURE_EPOCH_GAP > epoch_id && round < FUTURE_ROUND_GAP {
                    debug!(
                        "Overlord: state receive a future QC, epoch ID {}, round {}",
                        epoch_id, round,
                    );
                    self.votes.set_qc(aggregated_vote);
                } else {
                    warn!("Overlord: state receive a much higher aggregated vote");
                }
                return Ok(());
            }

            Ordering::Equal => (),
        }

        // State do not handle outdated prevote QC.
        if qc_type == VoteType::Prevote && round < self.round {
            debug!("Overlord: state receive a outdated prevote qc.");
            return Ok(());
        }

        trace::receive_vote(
            "receive_aggregated_vote".to_string(),
            epoch_id,
            round,
            hex::encode(aggregated_vote.leader.clone()),
            hex::encode(aggregated_vote.epoch_hash.clone()),
            Some(json!({ "qc type": qc_type.to_string() })),
        );

        // Verify aggregate signature and check the sum of the voting weights corresponding to the
        // hash exceeds the threshold.
        self.verify_aggregated_signature(
            aggregated_vote.signature.clone(),
            aggregated_vote.to_vote(),
            epoch_id,
            qc_type.clone(),
        )?;

        // Check if the epoch hash has been verified.
        let qc_hash = aggregated_vote.epoch_hash.clone();
        self.votes.set_qc(aggregated_vote);
        if !self.try_get_full_txs(&qc_hash) {
            return Ok(());
        }

        info!(
            "Overlord: state trigger SMR {:?} QC epoch ID {}, round {}",
            qc_type, self.epoch_id, self.round
        );

        self.state_machine.trigger(SMRTrigger {
            trigger_type: qc_type.clone().into(),
            source:       TriggerSource::State,
            hash:         qc_hash,
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
    /// certificates exists or not. If self node is not the leader, check if there is prevoteQC
    /// exits. If self node is the leader, check if there is signed prevote vote exists. It
    /// should be noted that when self is the leader, and the vote type is prevote, the process
    /// should be the same as the handle signed vote.
    async fn vote_process(&mut self, vote_type: VoteType) -> ConsensusResult<()> {
        if !self.is_leader {
            if let Ok(qc) = self
                .votes
                .get_qc_by_id(self.epoch_id, self.round, vote_type.clone())
            {
                let mut epoch_hash = qc.epoch_hash.clone();
                if !epoch_hash.is_empty() {
                    if vote_type == VoteType::Prevote {
                        match self.check_full_txs(epoch_hash) {
                            Some(tmp) => epoch_hash = tmp,
                            None => return Ok(()),
                        }
                    } else if !self.try_get_full_txs(&epoch_hash) {
                        return Ok(());
                    }
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

            if !epoch_hash.is_empty() {
                if vote_type == VoteType::Prevote {
                    match self.check_full_txs(epoch_hash) {
                        Some(tmp) => epoch_hash = tmp,
                        None => return Ok(()),
                    }
                } else if !self.try_get_full_txs(&epoch_hash) {
                    return Ok(());
                }
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
        let len = self
            .votes
            .vote_count(self.epoch_id, self.round, vote_type.clone());
        let vote_map = self
            .votes
            .get_vote_map(self.epoch_id, self.round, vote_type.clone())?;
        let threshold = self.authority.get_vote_weight_sum(true)? * 2;

        info!(
            "Overlord: state round {}, {:?} vote pool length {}",
            self.round, vote_type, len
        );

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
        let mut votes =
            self.votes
                .get_votes(self.epoch_id, self.round, vote_type.clone(), &epoch_hash)?;
        votes.sort();

        debug!("Overlord: state build aggregated signature");

        let len = votes.len();
        let mut signatures = Vec::with_capacity(len);
        let mut voters = Vec::with_capacity(len);
        for vote in votes.into_iter() {
            signatures.push(vote.signature);
            voters.push(vote.voter);
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
        for sv in votes.into_iter() {
            let signature = sv.signature.clone();
            let voter = sv.voter.clone();
            let vote = sv.vote.clone();

            if self
                .verify_signature(
                    self.util.hash(Bytes::from(encode(&vote))),
                    signature,
                    &voter,
                    MsgType::SignedVote,
                )
                .is_ok()
                && self.verify_address(&voter, true).is_ok()
            {
                self.votes.insert_vote(sv.get_hash(), sv, voter);
            }
        }
        Ok(())
    }

    fn re_check_qcs(&mut self, qcs: Vec<AggregatedVote>) -> ConsensusResult<()> {
        debug!("Overlord: state re-check future QCs");
        for qc in qcs.into_iter() {
            if self
                .verify_aggregated_signature(
                    qc.signature.clone(),
                    qc.to_vote(),
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

    /// If self is not the proposer of the epoch ID and round, set leader address as the proposer
    /// address.
    fn is_proposer(&mut self) -> ConsensusResult<bool> {
        let proposer = self
            .authority
            .get_proposer(self.epoch_id + self.round, true)?;

        if proposer == self.address {
            info!("Overlord: state self become leader");
            return Ok(true);
        }
        self.leader_address = proposer;
        Ok(false)
    }

    fn next_proposer(&self, seed: u64) -> ConsensusResult<bool> {
        let proposer = self.authority.get_proposer(seed, true)?;
        Ok(self.address == proposer)
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

        Ok(SignedVote {
            voter: self.address.clone(),
            signature,
            vote,
        })
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
        self.util
            .verify_signature(signature, hash, address.to_owned())
            .map_err(|err| {
                ConsensusError::CryptoErr(format!("{:?} signature error {:?}", msg_type, err))
            })?;
        Ok(())
    }

    fn verify_aggregated_signature(
        &self,
        signature: AggregatedSignature,
        vote: Vote,
        epoch_id: u64,
        vote_type: VoteType,
    ) -> ConsensusResult<()> {
        debug!("Overlord: state verify an aggregated signature");
        if !self
            .authority
            .is_above_threshold(&signature.address_bitmap, epoch_id == self.epoch_id)?
        {
            return Err(ConsensusError::AggregatedSignatureErr(format!(
                "{:?} QC of epoch {}, round {} is not above threshold",
                vote_type, self.epoch_id, self.round
            )));
        }

        let mut voters = self
            .authority
            .get_voters(&signature.address_bitmap, epoch_id == self.epoch_id)?;
        voters.sort();

        self.util
            .verify_aggregated_signature(
                signature.signature,
                self.util.hash(Bytes::from(encode(&vote))),
                voters,
            )
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
            .transmit_to_relayer(ctx, self.leader_address.clone(), msg.clone())
            .await
            .map_err(|err| {
                trace::error(
                    "transmit_message_to_leader".to_string(),
                    Some(json!({
                        "epoch_id": self.epoch_id,
                        "round": self.round,
                        "message type": msg.to_string(),
                    })),
                );

                error!(
                    "Overlord: state transmit message to leader failed {:?}",
                    err
                );
            });
    }

    async fn retransmit_qc(&self, ctx: Context, address: Address) -> ConsensusResult<()> {
        debug!("Overlord: state re-transmit last epoch vote");
        if let Some(qc) = self.last_commit_qc.clone() {
            let _ = self
                .function
                .transmit_to_relayer(ctx, address, OverlordMsg::AggregatedVote(qc))
                .await
                .map_err(|err| {
                    trace::error(
                        "retransmit_qc_to_leader".to_string(),
                        Some(json!({
                            "epoch_id": self.epoch_id,
                            "round": self.round,
                        })),
                    );

                    error!(
                        "Overlord: state transmit message to leader failed {:?}",
                        err
                    );
                });
        }
        Ok(())
    }

    async fn broadcast(&self, ctx: Context, msg: OverlordMsg<T>) {
        info!(
            "Overlord: state broadcast a message to others epoch ID {}, round {}",
            self.epoch_id, self.round
        );

        let _ = self
            .function
            .broadcast_to_other(ctx, msg.clone())
            .await
            .map_err(|err| {
                trace::error(
                    "broadcast_message".to_string(),
                    Some(json!({
                        "epoch_id": self.epoch_id,
                        "round": self.round,
                        "message type": msg.to_string(),
                    })),
                );

                error!("Overlord: state broadcast message failed {:?}", err);
            });
    }

    async fn check_epoch(&mut self, ctx: Context, hash: Hash, epoch: T) {
        let epoch_id = self.epoch_id;
        let function = Arc::clone(&self.function);
        let resp_tx = self.resp_tx.clone();

        trace::custom(
            "check_epoch".to_string(),
            Some(json!({
                "epoch ID": epoch_id,
                "round": self.round,
                "epoch hash": hex::encode(hash.clone())
            })),
        );

        runtime::spawn(async move {
            if let Err(e) =
                check_current_epoch(ctx, function, epoch_id, hash.clone(), epoch, resp_tx).await
            {
                trace::error(
                    "check_epoch".to_string(),
                    Some(json!({
                        "epoch_id": epoch_id,
                        "hash": hex::encode(hash),
                    })),
                );

                error!("Overlord: state check epoch failed: {:?}", e);
            }
        });
    }

    fn check_full_txs(&mut self, hash: Hash) -> Option<Hash> {
        if let Some(res) = self.is_full_transcation.get(&hash) {
            if *res {
                return Some(hash);
            }
        }
        None
    }

    fn try_get_full_txs(&self, hash: &Hash) -> bool {
        debug!("Overlord: state check if get full transcations");
        if let Some(res) = self.is_full_transcation.get(hash) {
            return *res;
        }
        false
    }

    /// Filter the proposals that do not need to be handed.
    /// 1. Outdated proposals
    /// 2. A much higher epoch ID which is larger than the FUTURE_EPOCH_GAP
    /// 3. A much higher round which is larger than the FUTURE_ROUND_GAP
    fn filter_signed_proposal(
        &mut self,
        epoch_id: u64,
        round: u64,
        signed_proposal: &SignedProposal<T>,
    ) -> ConsensusResult<bool> {
        if epoch_id < self.epoch_id - 1 || (epoch_id == self.epoch_id && round < self.round) {
            debug!(
                "Overlord: state receive an outdated signed proposal, epoch ID {}, round {}",
                epoch_id, round,
            );
            return Ok(true);
        } else if self.epoch_id + FUTURE_EPOCH_GAP < epoch_id {
            warn!("Overlord: state receive a much higher epoch's proposal.");
            return Ok(true);
        } else if (epoch_id == self.epoch_id && self.round + FUTURE_ROUND_GAP < round)
            || (epoch_id > self.epoch_id && round > FUTURE_ROUND_GAP)
        {
            warn!("Overlord: state receive a much higher round's proposal.");
            return Ok(true);
        }

        // If the proposal epoch ID is higher than the current epoch ID or proposal epoch ID is
        // equal to the current epoch ID and the proposal round is higher than the current round,
        // cache it until that epoch ID.
        if (epoch_id == self.epoch_id && round != self.round) || epoch_id > self.epoch_id {
            debug!(
                "Overlord: state receive a future signed proposal, epoch ID {}, round {}",
                epoch_id, round,
            );
            self.proposals
                .insert(epoch_id, round, signed_proposal.clone())?;
            return Ok(true);
        }
        Ok(false)
    }

    #[cfg(test)]
    pub fn set_condition(&mut self, epoch_id: u64, round: u64) {
        self.epoch_id = epoch_id;
        self.round = round;
    }

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
        self.is_full_transcation.insert(hash, true);
    }

    #[cfg(test)]
    pub fn set_hash_with_epoch(&mut self, hash_with_epoch: HashMap<Hash, T>) {
        self.hash_with_epoch = hash_with_epoch;
    }
}

async fn check_current_epoch<U: Consensus<T, S>, T: Codec, S: Codec>(
    ctx: Context,
    function: Arc<U>,
    epoch_id: u64,
    hash: Hash,
    epoch: T,
    tx: UnboundedSender<VerifyResp<S>>,
) -> ConsensusResult<()> {
    let txs = if let Ok(res) = function
        .check_epoch(ctx, epoch_id, hash.clone(), epoch)
        .await
    {
        Some(res)
    } else {
        None
    };

    info!("Overlord: state check epoch {}", txs.is_some());

    tx.unbounded_send(VerifyResp {
        epoch_id,
        epoch_hash: hash,
        full_txs: txs,
    })
    .map_err(|e| ConsensusError::ChannelErr(e.to_string()))?;
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
