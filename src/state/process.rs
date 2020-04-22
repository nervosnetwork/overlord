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

use crate::error::ConsensusError;
use crate::smr::smr_types::{FromWhere, SMREvent, SMRTrigger, Step, TriggerSource, TriggerType};
use crate::smr::{Event, SMRHandler};
use crate::state::collection::{ChokeCollector, ProposalCollector, VoteCollector};
use crate::types::{
    Address, AggregatedChoke, AggregatedSignature, AggregatedVote, Choke, Commit, Hash, Node,
    OverlordMsg, PoLC, Proof, Proposal, Signature, SignedChoke, SignedProposal, SignedVote, Status,
    UpdateFrom, VerifyResp, Vote, VoteType,
};
use crate::utils::auth_manage::AuthorityManage;
use crate::wal::{WalInfo, WalLock};
use crate::{Codec, Consensus, ConsensusResult, Crypto, Wal, INIT_HEIGHT, INIT_ROUND};

const FUTURE_HEIGHT_GAP: u64 = 5;
const FUTURE_ROUND_GAP: u64 = 10;

#[derive(Clone, Debug, Display, PartialEq, Eq)]
enum MsgType {
    #[display(fmt = "Signed Proposal message")]
    SignedProposal,

    #[display(fmt = "Signed Vote message")]
    SignedVote,
}

/// Overlord state struct. It maintains the local state of the node, and monitor the SMR event. The
/// `proposals` is used to cache the signed proposals that are with higher height or round. The
/// `hash_with_block` field saves hash and its corresponding block with the current height and
/// round. The `votes` field saves all signed votes and quorum certificates which height is higher
/// than `current_height - 1`.
#[derive(Debug)]
pub struct State<T: Codec, F: Consensus<T>, C: Crypto, W: Wal> {
    height:              u64,
    round:               u64,
    state_machine:       SMRHandler,
    address:             Address,
    proposals:           ProposalCollector<T>,
    votes:               VoteCollector,
    chokes:              ChokeCollector,
    authority:           AuthorityManage,
    hash_with_block:     HashMap<Hash, T>,
    is_full_transcation: HashMap<Hash, bool>,
    is_leader:           bool,
    leader_address:      Address,
    update_from_where:   UpdateFrom,
    height_start:        Instant,
    block_interval:      u64,
    consensus_power:     bool,
    stopped:             bool,

    resp_tx:  UnboundedSender<VerifyResp>,
    function: Arc<F>,
    wal:      Arc<W>,
    util:     Arc<C>,
}

impl<T, F, C, W> State<T, F, C, W>
where
    T: Codec + 'static,
    F: Consensus<T> + 'static,
    C: Crypto,
    W: Wal,
{
    /// Create a new state struct.
    pub(crate) fn new(
        smr: SMRHandler,
        addr: Address,
        interval: u64,
        mut authority_list: Vec<Node>,
        consensus: Arc<F>,
        crypto: Arc<C>,
        wal_engine: Arc<W>,
    ) -> (Self, UnboundedReceiver<VerifyResp>) {
        let (tx, rx) = unbounded();
        let mut auth = AuthorityManage::new();
        auth.update(&mut authority_list);

        let state = State {
            height:              INIT_HEIGHT,
            round:               INIT_ROUND,
            state_machine:       smr,
            address:             addr,
            proposals:           ProposalCollector::new(),
            votes:               VoteCollector::new(),
            chokes:              ChokeCollector::new(),
            authority:           auth,
            hash_with_block:     HashMap::new(),
            is_full_transcation: HashMap::new(),
            is_leader:           false,
            leader_address:      Address::default(),
            update_from_where:   UpdateFrom::PrecommitQC(mock_init_qc()),
            height_start:        Instant::now(),
            block_interval:      interval,
            consensus_power:     false,
            stopped:             false,

            resp_tx:  tx,
            function: consensus,
            util:     crypto,
            wal:      wal_engine,
        };

        (state, rx)
    }

    /// Run state module.
    pub(crate) async fn run(
        &mut self,
        mut raw_rx: UnboundedReceiver<(Context, OverlordMsg<T>)>,
        mut event: Event,
        mut verify_resp: UnboundedReceiver<VerifyResp>,
    ) {
        debug!("Overlord: state start running");
        if let Err(e) = self.start_with_wal().await {
            error!("Overlord: start with wal error {:?}", e);
        }

        loop {
            select! {
                raw = raw_rx.next() => {
                    if let Err(e) = self.handle_msg(raw).await {
                        error!("Overlord: state {:?} error", e);
                    }
                }
                evt = event.next() => {
                    if self.stopped {
                        break;
                    }

                    if !self.consensus_power {
                        continue;
                    }

                    if let Err(e) = self.handle_event(evt).await{
                        error!("Overlord: state {:?} error", e);
                    }
                }
                res = verify_resp.next() => {
                    if !self.consensus_power {
                        continue;
                    }

                    if let Err(e) = self.handle_resp(res) {
                        error!("Overlord: state {:?} error", e);
                    }
                }
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

        if !self.consensus_power && !raw.is_rich_status() {
            return Ok(());
        }

        match raw {
            OverlordMsg::SignedProposal(sp) => {
                if let Err(e) = self.handle_signed_proposal(ctx.clone(), sp).await {
                    trace::error(
                        "handle_signed_proposal".to_string(),
                        Some(json!({
                            "height": self.height,
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
                            "height": self.height,
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
                            "height": self.height,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle signed vote error {:?}", e);
                }
                Ok(())
            }

            OverlordMsg::SignedChoke(sc) => {
                if let Err(e) = self.handle_signed_choke(ctx.clone(), sc).await {
                    trace::error(
                        "handle_choke".to_string(),
                        Some(json!({
                            "height": self.height,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle signed choke error {:?}", e);
                }
                Ok(())
            }

            OverlordMsg::RichStatus(rs) => {
                if let Err(e) = self.goto_new_height(ctx.clone(), rs).await {
                    trace::error(
                        "goto new height".to_string(),
                        Some(json!({
                            "height": self.height,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle rich status error {:?}", e);
                }
                Ok(())
            }

            OverlordMsg::Stop => {
                self.state_machine.trigger(SMRTrigger {
                    trigger_type: TriggerType::Stop,
                    source:       TriggerSource::State,
                    hash:         Hash::new(),
                    round:        Some(self.round),
                    height:       self.height,
                    wal_info:     None,
                })?;
                self.stopped = true;
                Ok(())
            }

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
                from_where,
                ..
            } => {
                if let Err(e) = self
                    .handle_new_round(round, lock_round, lock_proposal, from_where)
                    .await
                {
                    trace::error(
                        "handle_new_round".to_string(),
                        Some(json!({
                            "height": self.height,
                            "round": round,
                            "is lock": lock_round.is_some(),
                        })),
                    );
                    error!("Overlord: state handle new round error {:?}", e);
                }
                Ok(())
            }

            SMREvent::PrevoteVote {
                block_hash,
                lock_round,
                ..
            } => {
                if let Err(e) = self
                    .handle_vote_event(block_hash, VoteType::Prevote, lock_round)
                    .await
                {
                    trace::error(
                        "handle_prevote_vote".to_string(),
                        Some(json!({
                            "height": self.height,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle prevote vote error {:?}", e);
                }
                Ok(())
            }

            SMREvent::PrecommitVote {
                block_hash,
                lock_round,
                ..
            } => {
                if let Err(e) = self
                    .handle_vote_event(block_hash, VoteType::Precommit, lock_round)
                    .await
                {
                    trace::error(
                        "handle_precommit_vote".to_string(),
                        Some(json!({
                            "height": self.height,
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
                            "height": self.height,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle commit error {:?}", e);
                }
                Ok(())
            }

            SMREvent::Brake {
                height,
                round,
                lock_round,
            } => {
                if height != self.height {
                    return Ok(());
                }

                if let Err(e) = self.handle_brake(round, lock_round).await {
                    trace::error(
                        "handle_brake_event".to_string(),
                        Some(json!({
                            "height": self.height,
                            "round": self.round,
                        })),
                    );

                    error!("Overlord: state handle brake error {:?}", e);
                }
                Ok(())
            }

            _ => unreachable!(),
        }
    }

    fn handle_resp(&mut self, msg: Option<VerifyResp>) -> ConsensusResult<()> {
        let resp = msg.ok_or_else(|| ConsensusError::Other("Event sender dropped".to_string()))?;
        if resp.height != self.height {
            return Ok(());
        }

        let block_hash = resp.block_hash.clone();
        info!(
            "Overlord: state receive a verify response true, height {}, round {}, hash {:?}",
            resp.height, resp.round, block_hash
        );

        trace::custom(
            "check_block_response".to_string(),
            Some(json!({
                "height": self.height,
                "hash": hex::encode(block_hash.clone()),
                "is_pass": true,
            })),
        );

        self.is_full_transcation
            .insert(block_hash.clone(), resp.is_pass);

        if let Some(qc) =
            self.votes
                .get_qc_by_hash(self.height, block_hash.clone(), VoteType::Precommit)
        {
            self.state_machine.trigger(SMRTrigger {
                trigger_type: TriggerType::PrecommitQC,
                source:       TriggerSource::State,
                hash:         qc.block_hash,
                round:        Some(self.round),
                height:       self.height,
                wal_info:     None,
            })?;
        } else if let Some(qc) =
            self.votes
                .get_qc_by_hash(self.height, block_hash, VoteType::Prevote)
        {
            if qc.round == self.round {
                self.state_machine.trigger(SMRTrigger {
                    trigger_type: TriggerType::PrevoteQC,
                    source:       TriggerSource::State,
                    hash:         qc.block_hash,
                    round:        Some(self.round),
                    height:       self.height,
                    wal_info:     None,
                })?;
            }
        }
        Ok(())
    }

    /// On receiving a rich status will call this method. This status can be either the return value
    /// of the `commit()` interface, or lastest status after the synchronization is completed send
    /// by the overlord handler.
    ///
    /// If the difference between the status height and current's over one, get the last authority
    /// list of the status height firstly. Then update the height, authority_list and the block
    /// interval. Since it is possible to have received and cached the current height's proposals,
    /// votes and quorum certificates before, these should be re-checked as goto new height.
    /// Finally, trigger SMR to goto new height.
    async fn goto_new_height(&mut self, _ctx: Context, status: Status) -> ConsensusResult<()> {
        if status.height <= self.height {
            warn!(
                "Overlord: state receive an outdated status, height {}, self height {}",
                status.height, self.height
            );
            return Ok(());
        }

        let new_height = status.height;
        self.height = new_height;
        self.round = INIT_ROUND;

        // Check the consensus power.
        self.consensus_power = status.is_consensus_node(&self.address);
        if !self.consensus_power {
            info!(
                "Overlord: self does not have consensus power height {}",
                new_height
            );
            return Ok(());
        }

        info!("Overlord: state goto new height {}", self.height);
        trace::start_block(new_height);
        self.save_wal(Step::Propose, None).await?;

        // Update height and authority list.
        self.height_start = Instant::now();
        let mut auth_list = status.authority_list.clone();
        self.authority.update(&mut auth_list);

        if let Some(interval) = status.interval {
            self.block_interval = interval;
        }

        // Clear outdated proposals and votes.
        self.proposals.flush(new_height - 1);
        self.votes.flush(new_height - 1);
        self.hash_with_block.clear();
        self.chokes.clear();

        // Re-check proposals that have been in the proposal collector, of the current height.
        if let Some(proposals) = self.proposals.get_height_proposals(self.height) {
            self.re_check_proposals(proposals)?;
        }

        // Re-check votes and quorum certificates in the vote collector, of the current height.
        if let Some((votes, qcs)) = self.votes.get_height_votes(new_height) {
            self.re_check_votes(votes)?;
            self.re_check_qcs(qcs)?;
        }

        self.state_machine.new_height_status(status.into())?;
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
        from_where: FromWhere,
    ) -> ConsensusResult<()> {
        info!("Overlord: state goto new round {}", round);
        trace::start_round(round, self.height);

        self.round = round;
        self.is_leader = false;

        if lock_round.is_some().bitxor(lock_proposal.is_some()) {
            return Err(ConsensusError::ProposalErr(
                "Lock round is inconsistent with lock proposal".to_string(),
            ));
        }

        self.set_update_from(from_where)?;
        self.save_wal_with_lock_round(Step::Propose, lock_round)
            .await?;

        // If self is not proposer, check whether it has received current signed proposal before. If
        // has, then handle it.
        if !self.is_proposer()? {
            if let Ok(signed_proposal) = self.proposals.get(self.height, self.round) {
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
        // block with its hash. These things consititute a Proposal. Then sign it and broadcast it
        // to other nodes.
        //
        // 2. Proposal with a lock
        // The case is much more complex. State should get the whole block and prevote quorum
        // certificate form proposal collector and vote collector. Some necessary checks should be
        // done by doing this. These things consititute a Proposal. Then sign it and broadcast it to
        // other nodes.
        trace::start_step("become_leader".to_string(), self.round, self.height);
        self.is_leader = true;
        let ctx = Context::new();
        let (block, hash, polc) = if lock_round.is_none() {
            let (new_block, new_hash) = self
                .function
                .get_block(ctx.clone(), self.height)
                .await
                .map_err(|err| ConsensusError::Other(format!("get block error {:?}", err)))?;
            (new_block, new_hash, None)
        } else {
            let round = lock_round.clone().unwrap();
            let hash = lock_proposal.unwrap();
            let block = self.hash_with_block.get(&hash).ok_or_else(|| {
                ConsensusError::ProposalErr(format!("Lose whole block that hash is {:?}", hash))
            })?;

            // Create PoLC by prevoteQC.
            let qc = self
                .votes
                .get_qc_by_id(self.height, round, VoteType::Prevote)
                .map_err(|err| ConsensusError::ProposalErr(format!("{:?} when propose", err)))?;
            let polc = PoLC {
                lock_round: round,
                lock_votes: qc,
            };
            (block.to_owned(), hash, Some(polc))
        };

        self.hash_with_block
            .entry(hash.clone())
            .or_insert_with(|| block.clone());

        let proposal = Proposal {
            height:     self.height,
            round:      self.round,
            content:    block.clone(),
            block_hash: hash.clone(),
            lock:       polc.clone(),
            proposer:   self.address.clone(),
        };

        info!(
            "Overlord: state broadcast a signed proposal height {}, round {}, hash {:?} and trigger SMR",
            self.height,
            self.round,
            hex::encode(hash.clone())
        );

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
            height:       self.height,
            wal_info:     None,
        })?;

        self.check_block(ctx, hash, block).await;
        // self.vote_process(VoteType::Prevote).await?;
        Ok(())
    }

    /// This function only handle signed proposals which height and round are equal to current.
    /// Others will be ignored or stored in the proposal collector.
    async fn handle_signed_proposal(
        &mut self,
        ctx: Context,
        signed_proposal: SignedProposal<T>,
    ) -> ConsensusResult<()> {
        let height = signed_proposal.proposal.height;
        let round = signed_proposal.proposal.round;

        info!(
            "Overlord: state receive a signed proposal height {}, round {}, from {:?}, hash {:?}",
            height,
            round,
            hex::encode(signed_proposal.proposal.proposer.clone()),
            hex::encode(signed_proposal.proposal.block_hash.clone())
        );

        if self.filter_signed_proposal(height, round, &signed_proposal)? {
            return Ok(());
        }

        trace::receive_proposal(
            "receive_signed_proposal".to_string(),
            height,
            round,
            hex::encode(signed_proposal.proposal.proposer.clone()),
            hex::encode(signed_proposal.proposal.block_hash.clone()),
            None,
        );

        //  Verify proposal signature.
        let proposal = signed_proposal.proposal.clone();
        let signature = signed_proposal.signature.clone();
        self.verify_proposer(height, round, &proposal.proposer)?;
        self.verify_signature(
            self.util.hash(Bytes::from(encode(&proposal))),
            signature,
            &proposal.proposer,
            MsgType::SignedProposal,
        )?;

        // If the signed proposal is with a lock, check the lock round and the QC then trigger it to
        // SMR. Otherwise, touch off SMR directly.
        let lock_round = if let Some(polc) = proposal.lock.clone() {
            debug!("Overlord: state receive a signed proposal with a lock");

            if !self
                .authority
                .is_above_threshold(&polc.lock_votes.signature.address_bitmap)?
            {
                return Err(ConsensusError::AggregatedSignatureErr(format!(
                    "aggregate signature below two thirds, proposal of height {:?}, round {:?}",
                    proposal.height, proposal.round
                )));
            }

            self.verify_aggregated_signature(
                polc.lock_votes.signature.clone(),
                polc.lock_votes.to_vote(),
                VoteType::Prevote,
            )
            .map_err(|err| {
                ConsensusError::AggregatedSignatureErr(format!(
                    "{:?} proposal of height {:?}, round {:?}",
                    err, proposal.height, proposal.round
                ))
            })?;
            Some(polc.lock_round)
        } else {
            None
        };

        let hash = proposal.block_hash.clone();
        let block = proposal.content.clone();
        self.hash_with_block.insert(hash.clone(), proposal.content);
        self.proposals
            .insert(self.height, self.round, signed_proposal)?;

        info!(
            "Overlord: state trigger SMR proposal height {}, round {}, hash {:?}",
            self.height,
            self.round,
            hex::encode(hash.clone())
        );

        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::Proposal,
            source:       TriggerSource::State,
            hash:         hash.clone(),
            round:        lock_round,
            height:       self.height,
            wal_info:     None,
        })?;

        debug!("Overlord: state check the whole block");
        self.check_block(ctx, hash, block).await;
        Ok(())
    }

    async fn handle_vote_event(
        &mut self,
        hash: Hash,
        vote_type: VoteType,
        lock_round: Option<u64>,
    ) -> ConsensusResult<()> {
        info!(
            "Overlord: state receive {:?} vote event height {}, round {}, hash {:?}",
            vote_type.clone(),
            self.height,
            self.round,
            hex::encode(hash.clone())
        );

        trace::custom(
            "receive_vote_event".to_string(),
            Some(json!({
                "height": self.height,
                "round": self.round,
                "vote type": vote_type.clone().to_string(),
                "vote hash": hex::encode(hash.clone()),
            })),
        );

        let signed_vote = self.sign_vote(Vote {
            height:     self.height,
            round:      self.round,
            vote_type:  vote_type.clone(),
            block_hash: hash.clone(),
        })?;

        self.save_wal_with_lock_round(vote_type.clone().into(), lock_round)
            .await?;

        if self.is_leader {
            self.votes
                .insert_vote(signed_vote.get_hash(), signed_vote, self.address.clone());
        } else {
            info!(
                "Overlord: state transmit a signed vote, height {}, round {}, hash {:?}",
                self.height,
                self.round,
                hex::encode(hash)
            );

            self.transmit(Context::new(), OverlordMsg::SignedVote(signed_vote))
                .await;
        }

        self.vote_process(vote_type).await?;
        Ok(())
    }

    async fn handle_brake(&mut self, round: u64, lock_round: Option<u64>) -> ConsensusResult<()> {
        if round != self.round {
            return Err(ConsensusError::CorrectnessErr(format!(
                "SMR round {}, state round {}",
                round, self.round
            )));
        }

        let choke = Choke {
            height: self.height,
            round:  self.round,
            from:   self.update_from_where.clone(),
        };

        let signature = self
            .util
            .sign(self.util.hash(Bytes::from(encode(&choke.to_hash()))))
            .map_err(|err| ConsensusError::CryptoErr(format!("sign choke error {:?}", err)))?;
        let signed_choke = SignedChoke {
            signature,
            choke,
            address: self.address.clone(),
        };

        info!(
            "Overlord: state broadcast a signed brake in height {}, round {}",
            self.height, self.round
        );

        self.chokes.insert(self.round, signed_choke.clone());
        self.save_wal_with_lock_round(Step::Brake, lock_round)
            .await?;
        self.broadcast(Context::new(), OverlordMsg::SignedChoke(signed_choke))
            .await;
        self.check_choke_above_threshold()?;
        Ok(())
    }

    async fn handle_commit(&mut self, hash: Hash) -> ConsensusResult<()> {
        info!(
            "Overlord: state receive commit event height {}, round {}, hash {:?}",
            self.height,
            self.round,
            hex::encode(hash.clone())
        );

        trace::custom(
            "receive_commit_event".to_string(),
            Some(json!({
                "height": self.height,
                "round": self.round,
                "block hash": hex::encode(hash.clone()),
            })),
        );

        debug!("Overlord: state get origin block");
        let height = self.height;
        let content = if let Some(tmp) = self.hash_with_block.get(&hash) {
            tmp.to_owned()
        } else {
            return Err(ConsensusError::Other(format!(
                "Lose whole block height {}, round {}",
                self.height, self.round
            )));
        };

        let qc = if let Some(tmp) =
            self.votes
                .get_qc_by_hash(height, hash.clone(), VoteType::Precommit)
        {
            tmp.to_owned()
        } else {
            return Err(ConsensusError::StorageErr("Lose precommit QC".to_string()));
        };

        let polc = Some(WalLock {
            lock_round: self.round,
            lock_votes: qc.clone(),
            content:    content.clone(),
        });
        self.save_wal(Step::Commit, polc).await?;

        debug!("Overlord: state generate proof");

        let proof = Proof {
            height,
            round: qc.round,
            block_hash: hash.clone(),
            signature: qc.signature.clone(),
        };
        let commit = Commit {
            height,
            content,
            proof,
        };

        let ctx = Context::new();
        let status = self
            .function
            .commit(ctx.clone(), height, commit)
            .await
            .map_err(|err| ConsensusError::Other(format!("commit error {:?}", err)))?;

        let mut auth_list = status.authority_list.clone();
        self.authority.update(&mut auth_list);
        let cost = Instant::now() - self.height_start;

        info!(
            "Overlord: achieve consensus in height {}, costs {} round {:?} time",
            self.height,
            self.round + 1,
            cost
        );

        if self.next_proposer(status.height, INIT_ROUND)?
            && cost < Duration::from_millis(self.block_interval)
        {
            Delay::new(Duration::from_millis(self.block_interval) - cost).await;
        }

        self.goto_new_height(ctx, status).await?;
        Ok(())
    }

    /// The main process of handle signed vote is that only handle those height and round are both
    /// equal to the current. The lower votes will be ignored directly even if the height is equal
    /// to the `current height - 1` and the round is higher than the current round. The reason is
    /// that the effective leader must in the lower height, and the task of handling signed votes
    /// will be done by the leader. For the higher votes, check the signature and save them in
    /// the vote collector. Whenevet the current vote is received, a statistic is made to check
    /// if the sum of the voting weights corresponding to the hash exceeds the threshold.
    async fn handle_signed_vote(
        &mut self,
        ctx: Context,
        signed_vote: SignedVote,
    ) -> ConsensusResult<()> {
        let height = signed_vote.get_height();
        let round = signed_vote.get_round();
        let vote_type = if signed_vote.is_prevote() {
            VoteType::Prevote
        } else {
            VoteType::Precommit
        };

        info!(
            "Overlord: state receive a signed {:?} vote height {}, round {}, from {:?}, hash {:?}",
            vote_type,
            height,
            round,
            hex::encode(signed_vote.voter.clone()),
            hex::encode(signed_vote.vote.block_hash.clone())
        );

        if self.filter_message(height, round) {
            return Ok(());
        }

        let tmp_type: String = vote_type.to_string();
        trace::receive_vote(
            "receive_signed_vote".to_string(),
            height,
            round,
            hex::encode(signed_vote.voter.clone()),
            hex::encode(signed_vote.vote.block_hash.clone()),
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
        self.verify_address(&voter)?;

        // Check if the quorum certificate has generated before check whether there is a hash that
        // vote weight is above the threshold. If no hash achieved this, return directly.
        if self
            .votes
            .get_qc_by_id(height, round, vote_type.clone())
            .is_ok()
        {
            return Ok(());
        }

        self.votes
            .insert_vote(signed_vote.get_hash(), signed_vote, voter);

        if height > self.height {
            return Ok(());
        }

        let block_hash = self.counting_vote(vote_type.clone())?;
        if block_hash.is_none() {
            debug!("Overlord: state counting of vote and no one above threshold");
            return Ok(());
        }

        // Build the quorum certificate needs to aggregate signatures into an aggregate
        // signature besides the address bitmap.
        let block_hash = block_hash.unwrap();
        let qc = self.generate_qc(block_hash.clone(), vote_type.clone())?;

        debug!(
            "Overlord: state set QC height {}, round {}",
            self.height, self.round
        );

        self.votes.set_qc(qc.clone());

        info!(
            "Overlord: state broadcast a {:?} QC, height {}, round {}, hash {:?}",
            vote_type,
            qc.height,
            qc.round,
            hex::encode(block_hash.clone())
        );

        self.broadcast(ctx, OverlordMsg::AggregatedVote(qc)).await;

        if !self.try_get_full_txs(&block_hash) {
            return Ok(());
        }

        info!(
            "Overlord: state trigger SMR {:?} QC height {}, round {}, hash {:?}",
            vote_type,
            self.height,
            self.round,
            hex::encode(block_hash.clone())
        );

        self.state_machine.trigger(SMRTrigger {
            trigger_type: vote_type.clone().into(),
            source:       TriggerSource::State,
            hash:         block_hash,
            round:        Some(round),
            height:       self.height,
            wal_info:     None,
        })?;
        Ok(())
    }

    /// The main process to handle aggregate votes contains four cases.
    ///
    /// 1. The QC is later than current which means the QC's height is higher than current or is
    /// equal to the current and the round is higher than current. In this cases, check the
    /// aggregate signature subject to availability, and save it.
    ///
    /// 2. The QC is equal to the current height and round. In this case, check the aggregate
    /// signature, then save it, and touch off SMR trigger.
    ///
    /// 3. The QC is equal to the `current height - 1` and the round is higher than the last
    /// commit round. In this case, check the aggregate signature firstly. If the type of the QC
    /// is precommit, ignore it. Otherwise, retransmit precommit QC.
    ///
    /// 4. Other cases, return `Ok(())` directly.
    async fn handle_aggregated_vote(
        &mut self,
        _ctx: Context,
        aggregated_vote: AggregatedVote,
    ) -> ConsensusResult<()> {
        let height = aggregated_vote.get_height();
        let round = aggregated_vote.get_round();
        let qc_type = if aggregated_vote.is_prevote_qc() {
            VoteType::Prevote
        } else {
            VoteType::Precommit
        };

        info!(
            "Overlord: state receive an {:?} QC height {}, round {}, from {:?}, hash {:?}",
            qc_type,
            height,
            round,
            hex::encode(aggregated_vote.leader.clone()),
            hex::encode(aggregated_vote.block_hash.clone())
        );

        // If the vote height is lower than the current height, ignore it directly. If the vote
        // height is higher than current height, save it and return Ok;
        match height.cmp(&self.height) {
            Ordering::Less => {
                debug!(
                    "Overlord: state receive an outdated QC, height {}, round {}",
                    height, round,
                );
                return Ok(());
            }

            Ordering::Greater => {
                if self.height + FUTURE_HEIGHT_GAP > height && round < FUTURE_ROUND_GAP {
                    debug!(
                        "Overlord: state receive a future QC, height {}, round {}",
                        height, round,
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
        } else if qc_type == VoteType::Precommit
            && aggregated_vote.block_hash.is_empty()
            && round < self.round
        {
            return Ok(());
        }

        trace::receive_vote(
            "receive_aggregated_vote".to_string(),
            height,
            round,
            hex::encode(aggregated_vote.leader.clone()),
            hex::encode(aggregated_vote.block_hash.clone()),
            Some(json!({ "qc type": qc_type.to_string() })),
        );

        // Verify aggregate signature and check the sum of the voting weights corresponding to the
        // hash exceeds the threshold.
        self.verify_aggregated_signature(
            aggregated_vote.signature.clone(),
            aggregated_vote.to_vote(),
            qc_type.clone(),
        )?;

        // Check if the block hash has been verified.
        let qc_hash = aggregated_vote.block_hash.clone();
        self.votes.set_qc(aggregated_vote);
        if !qc_hash.is_empty() && !self.try_get_full_txs(&qc_hash) {
            return Ok(());
        }

        info!(
            "Overlord: state trigger SMR {:?} QC height {}, round {}, hash {:?}",
            qc_type,
            self.height,
            self.round,
            hex::encode(qc_hash.clone())
        );

        self.state_machine.trigger(SMRTrigger {
            trigger_type: qc_type.into(),
            source:       TriggerSource::State,
            hash:         qc_hash,
            round:        Some(round),
            height:       self.height,
            wal_info:     None,
        })?;
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
                .get_qc_by_id(self.height, self.round, vote_type.clone())
            {
                let block_hash = qc.block_hash.clone();
                if !self.try_get_full_txs(&block_hash) {
                    return Ok(());
                }

                info!(
                    "Overlord: state trigger SMR height {}, round {}, type {:?}, hash {:?}",
                    self.height,
                    self.round,
                    qc.vote_type,
                    hex::encode(block_hash.clone())
                );

                self.state_machine.trigger(SMRTrigger {
                    trigger_type: qc.vote_type.into(),
                    source:       TriggerSource::State,
                    hash:         block_hash,
                    round:        Some(self.round),
                    height:       self.height,
                    wal_info:     None,
                })?;
                return Ok(());
            }
        } else if let Some(block_hash) = self.counting_vote(vote_type.clone())? {
            let qc = self.generate_qc(block_hash.clone(), vote_type.clone())?;
            self.votes.set_qc(qc.clone());

            info!(
                "Overlord: state broadcast a {:?} QC, height {}, round {}, hash {:?}",
                vote_type,
                qc.height,
                qc.round,
                hex::encode(block_hash.clone())
            );

            self.broadcast(Context::new(), OverlordMsg::AggregatedVote(qc))
                .await;

            if !self.try_get_full_txs(&block_hash) {
                return Ok(());
            }

            info!(
                "Overlord: state trigger SMR {:?} QC height {}, round {}, hash {:?}",
                vote_type,
                self.height,
                self.round,
                hex::encode(block_hash.clone())
            );

            self.state_machine.trigger(SMRTrigger {
                trigger_type: vote_type.clone().into(),
                source:       TriggerSource::State,
                hash:         block_hash,
                round:        Some(self.round),
                height:       self.height,
                wal_info:     None,
            })?;
        }
        Ok(())
    }

    fn counting_vote(&mut self, vote_type: VoteType) -> ConsensusResult<Option<Hash>> {
        let len = self
            .votes
            .vote_count(self.height, self.round, vote_type.clone());
        let vote_map = self
            .votes
            .get_vote_map(self.height, self.round, vote_type.clone())?;
        let threshold = self.authority.get_vote_weight_sum() * 2;

        info!(
            "Overlord: state round {}, {:?} vote pool length {}",
            self.round, vote_type, len
        );

        for (hash, set) in vote_map.iter() {
            let mut acc = 0u32;
            for addr in set.iter() {
                acc += self.authority.get_vote_weight(addr)?;
            }
            if u64::from(acc) * 3 > threshold {
                return Ok(Some(hash.to_owned()));
            }
        }
        Ok(None)
    }

    async fn handle_signed_choke(
        &mut self,
        ctx: Context,
        signed_choke: SignedChoke,
    ) -> ConsensusResult<()> {
        // verify signature
        let signature = signed_choke.signature.clone();
        let hash = self
            .util
            .hash(Bytes::from(encode(&signed_choke.choke.to_hash())));
        self.util
            .verify_signature(signature, hash, signed_choke.address.clone())
            .map_err(|err| ConsensusError::CryptoErr(format!("{:?}", err)))?;

        let choke = signed_choke.choke.clone();
        let choke_height = choke.height;
        let choke_round = choke.round;

        // filter choke height ne self.height
        if choke_height != self.height {
            return Ok(());
        }

        if choke_round < self.round {
            return Ok(());
        }

        info!(
            "Overlord: state receive a choke of height {}, round {}, from {:?}",
            choke_height,
            choke_round,
            hex::encode(signed_choke.address.clone())
        );

        if choke_round > self.round {
            match choke.from {
                UpdateFrom::PrevoteQC(qc) => {
                    return self.handle_aggregated_vote(ctx.clone(), qc).await
                }
                UpdateFrom::PrecommitQC(qc) => {
                    return self.handle_aggregated_vote(ctx.clone(), qc).await
                }
                UpdateFrom::ChokeQC(qc) => return self.handle_aggregated_choke(qc),
            }
        }

        self.chokes.insert(choke_round, signed_choke);
        self.check_choke_above_threshold()?;
        Ok(())
    }

    fn handle_aggregated_choke(
        &mut self,
        aggregated_choke: AggregatedChoke,
    ) -> ConsensusResult<()> {
        // verify is above threshold.
        if aggregated_choke.len() * 3 <= self.authority.len() * 2 {
            return Err(ConsensusError::BrakeErr(
                "choke qc is not above threshold".to_string(),
            ));
        }

        // verify aggregated signature.
        let choke = aggregated_choke.to_hash();
        let choke_hash = self.util.hash(Bytes::from(encode(&choke)));
        let voters = aggregated_choke.voters.clone();
        self.util
            .verify_aggregated_signature(aggregated_choke.signature.clone(), choke_hash, voters)
            .map_err(|err| {
                ConsensusError::CryptoErr(format!("choke qc signature error {:?}", err))
            })?;
        self.chokes.set_qc(choke.round, aggregated_choke);

        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::ContinueRound,
            source:       TriggerSource::State,
            hash:         Hash::new(),
            round:        Some(choke.round + 1),
            height:       self.height,
            wal_info:     None,
        })?;
        Ok(())
    }

    fn generate_qc(
        &mut self,
        block_hash: Hash,
        vote_type: VoteType,
    ) -> ConsensusResult<AggregatedVote> {
        let mut votes =
            self.votes
                .get_votes(self.height, self.round, vote_type.clone(), &block_hash)?;
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
        let mut bit_map = BitVec::from_elem(self.authority.len(), false);
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
            height: self.height,
            round: self.round,
            block_hash,
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
                .verify_proposer(proposal.height, proposal.round, &proposal.proposer)
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
                self.proposals.insert(proposal.height, proposal.round, sp)?;
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
                && self.verify_address(&voter).is_ok()
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
                    qc.vote_type.clone(),
                )
                .is_ok()
            {
                self.votes.set_qc(qc);
            }
        }
        Ok(())
    }

    /// If self is not the proposer of the height and round, set leader address as the proposer
    /// address.
    fn is_proposer(&mut self) -> ConsensusResult<bool> {
        let proposer = self.authority.get_proposer(self.height, self.round)?;

        if proposer == self.address {
            info!(
                "Overlord: state self become leader, height {}, round {}",
                self.height, self.round
            );
            return Ok(true);
        }

        info!(
            "Overlord: {:?} become leader, height {}, round {}",
            hex::encode(proposer.clone()),
            self.height,
            self.round
        );
        self.leader_address = proposer;
        Ok(false)
    }

    fn next_proposer(&self, height: u64, round: u64) -> ConsensusResult<bool> {
        let proposer = self.authority.get_proposer(height, round)?;
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
        let pretty_voter = voters
            .iter()
            .map(|addr| hex::encode(addr.clone()))
            .collect::<Vec<_>>();

        info!(
            "Overlord: state aggregate signatures height {}, round {}, voters {:?}",
            self.height, self.round, pretty_voter
        );

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
        vote_type: VoteType,
    ) -> ConsensusResult<()> {
        debug!("Overlord: state verify an aggregated signature");
        if !self
            .authority
            .is_above_threshold(&signature.address_bitmap)?
        {
            return Err(ConsensusError::AggregatedSignatureErr(format!(
                "{:?} QC of height {}, round {} is not above threshold",
                vote_type, self.height, self.round
            )));
        }

        let mut voters = self.authority.get_voters(&signature.address_bitmap)?;
        voters.sort();

        let pretty_voter = voters
            .iter()
            .map(|addr| hex::encode(addr.clone()))
            .collect::<Vec<_>>();

        info!(
            "Overlord: state verify aggregated signature, height {}, round {}, voters {:?}",
            self.height, self.round, pretty_voter
        );

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

    fn verify_proposer(&self, height: u64, round: u64, address: &Address) -> ConsensusResult<()> {
        debug!("Overlord: state verify a proposer");
        self.verify_address(address)?;
        if address != &self.authority.get_proposer(height, round)? {
            return Err(ConsensusError::ProposalErr("Invalid proposer".to_string()));
        }
        Ok(())
    }

    /// Check whether the given address is included in the corresponding authority list.
    fn verify_address(&self, address: &Address) -> ConsensusResult<()> {
        if !self.authority.contains(address) {
            return Err(ConsensusError::InvalidAddress);
        }
        Ok(())
    }

    async fn transmit(&self, ctx: Context, msg: OverlordMsg<T>) {
        debug!(
            "Overlord: state transmit a message to leader height {}, round {}",
            self.height, self.round
        );

        let _ = self
            .function
            .transmit_to_relayer(ctx, self.leader_address.clone(), msg.clone())
            .await
            .map_err(|err| {
                trace::error(
                    "transmit_message_to_leader".to_string(),
                    Some(json!({
                        "height": self.height,
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

    async fn broadcast(&self, ctx: Context, msg: OverlordMsg<T>) {
        debug!(
            "Overlord: state broadcast a message to others height {}, round {}",
            self.height, self.round
        );

        let _ = self
            .function
            .broadcast_to_other(ctx, msg.clone())
            .await
            .map_err(|err| {
                trace::error(
                    "broadcast_message".to_string(),
                    Some(json!({
                        "height": self.height,
                        "round": self.round,
                        "message type": msg.to_string(),
                    })),
                );

                error!("Overlord: state broadcast message failed {:?}", err);
            });
    }

    fn check_choke_above_threshold(&mut self) -> ConsensusResult<()> {
        self.chokes.print_round_choke_log(self.round);
        if let Some(round) = self.chokes.max_round_above_threshold(self.authority.len()) {
            if round < self.round {
                return Ok(());
            }

            info!("Overlord: round {} chokes above threshold", round);

            // aggregate chokes.
            let signed_chokes = self.chokes.get_chokes(round).unwrap();
            let mut sigs = Vec::with_capacity(signed_chokes.len());
            let mut voters = Vec::with_capacity(signed_chokes.len());
            for sc in signed_chokes.iter() {
                sigs.push(sc.signature.clone());
                voters.push(sc.address.clone());
            }
            let sig = self.aggregate_signatures(sigs, voters.clone())?;
            self.chokes.set_qc(round, AggregatedChoke {
                height: self.height,
                signature: sig,
                round,
                voters,
            });

            info!(
                "Overlord: state trigger SMR go on {} round of height {}",
                round + 1,
                self.height
            );

            self.state_machine.trigger(SMRTrigger {
                trigger_type: TriggerType::ContinueRound,
                source:       TriggerSource::State,
                hash:         Hash::new(),
                round:        Some(round + 1),
                height:       self.height,
                wal_info:     None,
            })?;
        }
        Ok(())
    }

    async fn check_block(&mut self, ctx: Context, hash: Hash, block: T) {
        let height = self.height;
        let round = self.round;
        let function = Arc::clone(&self.function);
        let resp_tx = self.resp_tx.clone();

        trace::custom(
            "check_block".to_string(),
            Some(json!({
                "height": height,
                "round": self.round,
                "block hash": hex::encode(hash.clone())
            })),
        );

        tokio::spawn(async move {
            if let Err(e) =
                check_current_block(ctx, function, height, round, hash.clone(), block, resp_tx)
                    .await
            {
                trace::error(
                    "check_block".to_string(),
                    Some(json!({
                        "height": height,
                        "hash": hex::encode(hash),
                    })),
                );

                error!("Overlord: state check block failed: {:?}", e);
            }
        });
    }

    async fn save_wal(&mut self, step: Step, lock: Option<WalLock<T>>) -> ConsensusResult<()> {
        let wal_info = WalInfo {
            height: self.height,
            round: self.round,
            step: step.clone(),
            from: self.update_from_where.clone(),
            lock,
        };

        self.wal
            .save(Bytes::from(encode(&wal_info)))
            .await
            .map_err(|e| {
                trace::error(
                    "save_wal".to_string(),
                    Some(json!({
                        "height": self.height,
                        "round": self.round,
                        "step": step.to_string(),
                        "error": e.to_string(),
                    })),
                );

                error!("Overlord: state save wal error {:?}", e);
                ConsensusError::SaveWalErr {
                    height: self.height,
                    round:  self.round,
                    step:   step.to_string(),
                }
            })?;
        Ok(())
    }

    async fn save_wal_with_lock_round(
        &mut self,
        step: Step,
        lock_round: Option<u64>,
    ) -> ConsensusResult<()> {
        let polc = if let Some(round) = lock_round {
            if let Ok(qc) = self
                .votes
                .get_qc_by_id(self.height, round, VoteType::Prevote)
            {
                let block = self
                    .hash_with_block
                    .get(&qc.block_hash)
                    .ok_or_else(|| ConsensusError::Other("lose whole block".to_string()))?;

                Some(WalLock {
                    lock_round: round,
                    lock_votes: qc,
                    content:    block.clone(),
                })
            } else {
                return Err(ConsensusError::Other("no qc".to_string()));
            }
        } else {
            None
        };

        self.save_wal(step, polc).await?;
        Ok(())
    }

    async fn start_with_wal(&mut self) -> ConsensusResult<()> {
        let wal_info = self.load_wal().await?;
        if wal_info.is_none() {
            return Ok(());
        }

        let wal_info = wal_info.unwrap();
        info!("overlord: start from wal {}", wal_info);

        // recover basic state
        self.consensus_power = true;
        self.height = wal_info.height;
        self.round = wal_info.round;
        self.is_leader = self.is_proposer()?;
        self.update_from_where = wal_info.from.clone();

        // recover lock state
        if wal_info.lock.is_some() {
            let lock = wal_info.lock.clone().unwrap();
            let qc = lock.lock_votes.clone();
            self.votes.set_qc(qc.clone());
            self.hash_with_block.insert(qc.block_hash, lock.content);
        }

        if wal_info.step == Step::Commit {
            let qc = wal_info
                .lock
                .clone()
                .ok_or_else(|| ConsensusError::LoadWalErr("no lock in commit step".to_string()))?;
            return self.handle_commit(qc.lock_votes.block_hash.clone()).await;
        }

        if wal_info.step == Step::Brake {
            let lock_round = wal_info.lock.clone().map(|lock| lock.lock_round);
            self.handle_brake(self.round, lock_round).await?;
        }

        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::WalInfo,
            source:       TriggerSource::State,
            hash:         Hash::new(),
            round:        None,
            height:       self.height,
            wal_info:     Some(wal_info.into_smr_base()),
        })?;
        Ok(())
    }

    async fn load_wal(&mut self) -> ConsensusResult<Option<WalInfo<T>>> {
        let tmp = self
            .wal
            .load()
            .await
            .map_err(|e| ConsensusError::LoadWalErr(e.to_string()))?;

        if tmp.is_none() {
            return Ok(None);
        }

        let info: WalInfo<T> = rlp::decode(tmp.unwrap().as_ref())
            .map_err(|e| ConsensusError::LoadWalErr(e.to_string()))?;
        Ok(Some(info))
    }

    /// When block hash is empty, return true directly.
    fn try_get_full_txs(&self, hash: &Hash) -> bool {
        debug!("Overlord: state check if get full transcations");
        if hash.is_empty() {
            return true;
        } else if let Some(res) = self.is_full_transcation.get(hash) {
            return *res;
        }
        false
    }

    fn set_update_from(&mut self, from_where: FromWhere) -> ConsensusResult<()> {
        let update_from = match from_where {
            FromWhere::PrevoteQC(round) => {
                let qc = self
                    .votes
                    .get_qc_by_id(self.height, round, VoteType::Prevote)?;
                UpdateFrom::PrevoteQC(qc)
            }

            FromWhere::PrecommitQC(round) => {
                let qc = if round == u64::max_value() {
                    mock_init_qc()
                } else {
                    self.votes
                        .get_qc_by_id(self.height, round, VoteType::Precommit)?
                };
                UpdateFrom::PrecommitQC(qc)
            }

            FromWhere::ChokeQC(round) => {
                let qc = self.chokes.get_qc(round).ok_or_else(|| {
                    ConsensusError::BrakeErr(format!(
                        "no choke qc height {} round {}",
                        self.height, round
                    ))
                })?;
                UpdateFrom::ChokeQC(qc)
            }
        };
        self.update_from_where = update_from;
        Ok(())
    }

    /// Filter the proposals that do not need to be handed.
    /// 1. Outdated proposals
    /// 2. A much higher height which is larger than the FUTURE_HEIGHT_GAP
    /// 3. A much higher round which is larger than the FUTURE_ROUND_GAP
    fn filter_signed_proposal(
        &mut self,
        height: u64,
        round: u64,
        signed_proposal: &SignedProposal<T>,
    ) -> ConsensusResult<bool> {
        if self.filter_message(height, round) {
            return Ok(true);
        }

        // If the proposal height is higher than the current height or proposal height is
        // equal to the current height and the proposal round is higher than the current round,
        // cache it until that height.
        if (height == self.height && round != self.round) || height > self.height {
            debug!(
                "Overlord: state receive a future signed proposal, height {}, round {}",
                height, round,
            );
            self.proposals
                .insert(height, round, signed_proposal.clone())?;
            return Ok(true);
        }
        Ok(false)
    }

    fn filter_message(&self, height: u64, round: u64) -> bool {
        if height < self.height || (height == self.height && round < self.round) {
            debug!(
                "Overlord: state receive an outdated message height {}, self height {}",
                height, self.height
            );
            return true;
        } else if self.height + FUTURE_HEIGHT_GAP < height {
            debug!(
                "Overlord: state receive a future message height {}, self height {}",
                height, self.height
            );
            return true;
        } else if (height == self.height && self.round + FUTURE_ROUND_GAP < round)
            || (height > self.height && round > FUTURE_ROUND_GAP)
        {
            debug!("Overlord: state receive a much higher round message");
            return true;
        }

        false
    }
}

async fn check_current_block<U: Consensus<T>, T: Codec>(
    ctx: Context,
    function: Arc<U>,
    height: u64,
    round: u64,
    hash: Hash,
    block: T,
    tx: UnboundedSender<VerifyResp>,
) -> ConsensusResult<()> {
    function
        .check_block(ctx, height, hash.clone(), block)
        .await
        .map_err(|err| ConsensusError::Other(format!("check {} block error {:?}", height, err)))?;

    debug!("Overlord: state check block {}", true);
    tx.unbounded_send(VerifyResp {
        height,
        round,
        block_hash: hash,
        is_pass: true,
    })
    .map_err(|e| ConsensusError::ChannelErr(e.to_string()))
}

fn mock_init_qc() -> AggregatedVote {
    let aggregated_signature = AggregatedSignature {
        signature:      Signature::default(),
        address_bitmap: Bytes::default(),
    };

    AggregatedVote {
        signature:  aggregated_signature,
        vote_type:  VoteType::Precommit,
        height:     0u64,
        round:      0u64,
        block_hash: Hash::default(),
        leader:     Address::default(),
    }
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
                "height": 1u64,
                "consensus_round": 1u64,
                "consume": cost,
            })
        );
    }
}
