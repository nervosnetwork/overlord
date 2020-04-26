use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::{select, StreamExt};
use log::{debug, error, info, warn};

use crate::error::ErrorKind;
use crate::state::{ProposePrepare, Stage, StateInfo, Step};
use crate::types::{
    Choke, ChokeQC, FetchedFullBlock, FullBlockWithProof, PreCommitQC, PreVoteQC, Proposal,
    SignedChoke, SignedHeight, SignedPreCommit, SignedPreVote, SignedProposal, SyncRequest,
    SyncResponse, UpdateFrom, Vote,
};
use crate::utils::agent::EventAgent;
use crate::utils::auth::{AuthCell, AuthFixedConfig, AuthManage};
use crate::utils::cabinet::{Cabinet, Capsule};
use crate::utils::exec::ExecRequest;
use crate::utils::sync::{Sync, SyncStat, BLOCK_BATCH};
use crate::utils::timeout::TimeoutEvent;
use crate::{
    Adapter, Address, Blk, ExecResult, Hash, Height, HeightRange, OverlordError, OverlordMsg,
    OverlordResult, Proof, Round, St, TinyHex, Wal, INIT_ROUND,
};

const HEIGHT_WINDOW: Height = 5;
const ROUND_WINDOW: Round = 5;

pub type WrappedOverlordMsg<B> = (Context, OverlordMsg<B>);

/// State Machine Replica
pub struct SMR<A: Adapter<B, S>, B: Blk, S: St> {
    address: Address,
    state:   StateInfo<B>,
    prepare: ProposePrepare<S>,
    sync:    Sync<B>,

    adapter: Arc<A>,
    wal:     Wal,
    cabinet: Cabinet<B>,
    auth:    AuthManage<A, B, S>,
    agent:   EventAgent<A, B, S>,

    phantom_s: PhantomData<S>,
}

impl<A, B, S> SMR<A, B, S>
where
    A: Adapter<B, S>,
    B: Blk,
    S: St,
{
    pub async fn new(
        auth_fixed_config: AuthFixedConfig,
        adapter: &Arc<A>,
        from_net: UnboundedReceiver<(Context, OverlordMsg<B>)>,
        to_net: UnboundedSender<(Context, OverlordMsg<B>)>,
        from_exec: UnboundedReceiver<ExecResult<S>>,
        to_exec: UnboundedSender<ExecRequest<S>>,
        wal_path: &str,
    ) -> Self {
        let address = &auth_fixed_config.address.clone();

        let wal = Wal::new(wal_path);
        let rst = wal.load_state();

        let state;
        let from;
        if let Err(e) = rst {
            warn!("Load state from wal failed: {}! Try to recover state by the adapter, which face security risk if majority auth nodes lost their wal file at the same time", e);
            state = recover_state_by_adapter(adapter).await;
            from = "adapter";
        } else {
            state = rst.unwrap();
            from = "wal"
        };

        let prepare = recover_propose_prepare_and_config(adapter).await;
        let last_exec_result = prepare.last_commit_exec_result.clone();
        let auth_config = last_exec_result.consensus_config.auth_config.clone();
        let time_config = last_exec_result.consensus_config.time_config.clone();
        let last_config = if prepare.exec_height > 0 {
            Some(
                get_exec_result(adapter, prepare.exec_height - 1)
                    .await
                    .consensus_config,
            )
        } else {
            None
        };

        let mut cabinet = Cabinet::default();
        let rst = wal.load_full_blocks();
        if let Ok(full_blocks) = rst {
            full_blocks
                .into_iter()
                .for_each(|fb| cabinet.insert_full_block(fb));
        } else {
            error!("Load full_block from wal failed! Have security risk if majority auth nodes lost their wal file at the same time");
        }

        let current_auth = AuthCell::new(auth_config, &address);
        let last_auth: Option<AuthCell<B>> =
            last_config.map(|config| AuthCell::new(config.auth_config, &address));

        info!(
            "[LOAD]\n\t<{}> <- {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n\n",
            address.tiny_hex(),
            from,
            state,
            prepare,
            Sync::<B>::new(),
        );
        SMR {
            wal,
            state,
            cabinet,
            prepare,
            phantom_s: PhantomData,
            address: address.clone(),
            sync: Sync::new(),
            adapter: Arc::<A>::clone(adapter),
            auth: AuthManage::new(auth_fixed_config, current_auth, last_auth),
            agent: EventAgent::new(
                address.clone(),
                adapter,
                time_config,
                from_net,
                to_net,
                from_exec,
                to_exec,
            ),
        }
    }

    pub async fn run(mut self) {
        self.agent.set_step_timeout(self.state.stage.clone());
        self.agent.set_height_timeout();

        loop {
            select! {
                opt = self.agent.from_net.next() => {
                    let msg = opt.expect("Net Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.handle_msg(msg.clone()).await {
                        let sender = self.extract_sender(&msg.1);
                        let content = format!("[RECEIVE]\n\t<{}> <- {}\n\t<message> {}\n\t{}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
                            self.address.tiny_hex(), sender.tiny_hex(), msg.1, e, self.state, self.prepare, self.sync);
                        self.handle_err(e, content).await;
                    }
                }
                opt = self.agent.from_exec.next() => {
                    self.handle_exec_result(opt.expect("Exec Channel is down! It's meaningless to continue running"));
                }
                opt = self.agent.from_fetch.next() => {
                    let fetch = opt.expect("Fetch Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.handle_fetch(fetch).await {
                        let content = format!("[FETCH]\n\t<{}> <- full block\n\t{}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n",
                            self.address.tiny_hex(), e, self.state, self.prepare, self.sync);
                        self.handle_err(e, content).await;
                    }
                }
                opt = self.agent.from_timeout.next() => {
                    let timeout = opt.expect("Timeout Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.handle_timeout(timeout.clone()).await {
                        let content = format!("[TIMEOUT]\n\t<{}> <- timeout\n\t<timeout> {}\n\t{}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
                            self.address.tiny_hex(), timeout, e, self.state, self.prepare, self.sync);
                        self.handle_err(e, content).await;
                    }
                }
            }
        }
    }

    async fn handle_msg(&mut self, wrapped_msg: WrappedOverlordMsg<B>) -> OverlordResult<()> {
        let (_, msg) = wrapped_msg;

        match msg {
            OverlordMsg::SignedProposal(signed_proposal) => {
                self.handle_signed_proposal(signed_proposal).await?;
            }
            OverlordMsg::SignedPreVote(signed_pre_vote) => {
                self.handle_signed_pre_vote(signed_pre_vote).await?;
            }
            OverlordMsg::SignedPreCommit(signed_pre_commit) => {
                self.handle_signed_pre_commit(signed_pre_commit).await?;
            }
            OverlordMsg::SignedChoke(signed_choke) => {
                self.handle_signed_choke(signed_choke).await?;
            }
            OverlordMsg::PreVoteQC(pre_vote_qc) => {
                self.handle_pre_vote_qc(pre_vote_qc, true).await?;
            }
            OverlordMsg::PreCommitQC(pre_commit_qc) => {
                self.handle_pre_commit_qc(pre_commit_qc, true).await?;
            }
            OverlordMsg::SignedHeight(signed_height) => {
                self.handle_signed_height(signed_height).await?;
            }
            OverlordMsg::SyncRequest(request) => {
                self.handle_sync_request(request).await?;
            }
            OverlordMsg::SyncResponse(response) => {
                self.handle_sync_response(response).await?;
            }
            OverlordMsg::Stop => {
                panic!(
                    "[STOP]\n\t<{}> -> heaven\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n\n",
                    self.address.tiny_hex(),
                    self.state,
                    self.prepare,
                    self.sync
                );
            }
        }

        Ok(())
    }

    fn handle_exec_result(&mut self, exec_result: ExecResult<S>) {
        let old_prepare = self.prepare.handle_exec_result(exec_result.clone());
        info!(
            "[EXEC]\n\t<{}> <- exec\n\t<message> exec_result: {}\n\t<state> {}\n\t<prepare> {} => {}\n\t<sync> {}\n\n",
            self.address.tiny_hex(),
            exec_result,
            self.state,
            old_prepare,
            self.prepare,
            self.sync
        );
    }

    async fn handle_fetch(
        &mut self,
        fetch_result: OverlordResult<FetchedFullBlock>,
    ) -> OverlordResult<()> {
        let fetch = self.agent.handle_fetch(fetch_result)?;
        if fetch.height < self.state.stage.height {
            return Err(OverlordError::debug_old());
        }
        self.cabinet.insert_full_block(fetch.clone());
        self.wal.save_full_block(&fetch)?;

        info!(
            "[FETCH]\n\t<{}> <- full block\n\t<message> full_block: {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n",
            self.address.tiny_hex(),
            fetch,
            self.state,
            self.prepare,
            self.sync
        );

        let hash = fetch.block_hash;
        if let Some(qc) = self.cabinet.get_pre_commit_qc_by_hash(fetch.height, &hash) {
            self.handle_pre_commit_qc(qc, false).await?;
        } else if let Some(qc) = self.cabinet.get_pre_vote_qc_by_hash(fetch.height, &hash) {
            self.handle_pre_vote_qc(qc, false).await?;
        }

        Ok(())
    }

    async fn handle_timeout(&mut self, timeout_event: TimeoutEvent) -> OverlordResult<()> {
        match timeout_event {
            TimeoutEvent::ProposeTimeout(stage) => self.handle_propose_timeout(stage).await,
            TimeoutEvent::PreVoteTimeout(stage) => self.handle_pre_vote_timeout(stage).await,
            TimeoutEvent::PreCommitTimeout(stage) => self.handle_pre_commit_timeout(stage).await,
            TimeoutEvent::BrakeTimeout(stage) => self.handle_brake_timeout(stage).await,
            TimeoutEvent::NextHeightTimeout => self.handle_next_height_timeout().await,
            TimeoutEvent::HeightTimeout => self.handle_height_timeout().await,
            TimeoutEvent::SyncTimeout(request_id) => {
                let old_sync = self.sync.handle_sync_timeout(request_id)?;
                self.log_sync_update_of_timeout(old_sync, TimeoutEvent::SyncTimeout(request_id));
                Ok(())
            }
            TimeoutEvent::ClearTimeout(address) => {
                let old_sync = self.sync.handle_clear_timeout(&address);
                self.log_sync_update_of_timeout(old_sync, TimeoutEvent::ClearTimeout(address));
                Ok(())
            }
        }
    }

    async fn handle_propose_timeout(&mut self, stage: Stage) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.into());
        self.agent.set_step_timeout(self.state.stage.clone());
        self.wal.save_state(&self.state)?;

        let height = self.state.stage.height;
        let round = self.state.stage.round;
        let vote = Vote::empty_vote(height, round);
        let signed_vote = self.auth.sign_pre_vote(vote)?;
        let leader = self.auth.get_leader(height, round);
        self.agent.transmit(leader, signed_vote.into()).await
    }

    async fn handle_pre_vote_timeout(&mut self, stage: Stage) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.into());
        self.agent.set_step_timeout(self.state.stage.clone());
        self.wal.save_state(&self.state)?;

        let height = self.state.stage.height;
        let round = self.state.stage.round;
        let vote = if let Some(lock) = self.state.lock.as_ref() {
            Vote::new(height, round, lock.vote.block_hash.clone())
        } else {
            Vote::empty_vote(height, round)
        };
        let signed_vote = self.auth.sign_pre_commit(vote)?;
        let leader = self.auth.get_leader(height, round);
        self.agent
            .transmit(leader, signed_vote.clone().into())
            .await?;
        // new design, when leader is down, next leader can aggregate votes to view change fast
        let next_leader = self.auth.get_leader(height, round + 1);
        self.agent.transmit(next_leader, signed_vote.into()).await?;
        Ok(())
    }

    async fn handle_pre_commit_timeout(&mut self, stage: Stage) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.into());
        self.agent.set_step_timeout(self.state.stage.clone());
        self.wal.save_state(&self.state)?;

        let height = self.state.stage.height;
        let round = self.state.stage.round;
        let from = self.state.from.clone();
        let choke = Choke::new(height, round);
        let signed_choke = self.auth.sign_choke(choke, from)?;
        self.agent.broadcast(signed_choke.into()).await?;
        Ok(())
    }

    async fn handle_brake_timeout(&mut self, stage: Stage) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.into());
        self.agent.set_step_timeout(self.state.stage.clone());

        let height = self.state.stage.height;
        let round = self.state.stage.round;
        let from = self.state.from.clone();
        let choke = Choke::new(height, round);
        let signed_choke = self.auth.sign_choke(choke, from)?;
        self.agent.broadcast(signed_choke.into()).await?;
        Ok(())
    }

    async fn handle_next_height_timeout(&mut self) -> OverlordResult<()> {
        self.next_height().await
    }

    async fn handle_signed_proposal(&mut self, sp: SignedProposal<B>) -> OverlordResult<()> {
        let msg_h = sp.proposal.height;
        let msg_r = sp.proposal.round;

        self.filter_msg(msg_h, msg_r, (&sp).into())?;
        // only msg of current height will go down
        self.check_proposal(&sp.proposal)?;
        self.auth.verify_signed_proposal(&sp)?;
        self.cabinet.insert(msg_h, msg_r, (&sp).into())?;

        self.check_block(&sp.proposal.block).await?;
        self.agent.request_full_block(sp.proposal.block.clone());

        if msg_r > self.state.stage.round {
            if let Some(qc) = sp.proposal.lock {
                self.handle_pre_vote_qc(qc, true).await?;
            }
            return Err(OverlordError::debug_high());
        }

        let old_state = self.state.handle_signed_proposal(&sp)?;
        self.log_state_update_of_msg(old_state, &sp.proposal.proposer, (&sp).into());
        self.agent.set_step_timeout(self.state.stage.clone());
        self.wal.save_state(&self.state)?;

        let vote = self.auth.sign_pre_vote(sp.proposal.as_vote())?;
        self.agent
            .transmit(sp.proposal.proposer, vote.clone().into())
            .await?;
        let next_leader = self.auth.get_leader(msg_h, msg_r + 1);
        self.agent.transmit(next_leader, vote.into()).await
    }

    async fn handle_signed_pre_vote(&mut self, sv: SignedPreVote) -> OverlordResult<()> {
        let msg_h = sv.vote.height;
        let msg_r = sv.vote.round;

        self.filter_msg(msg_h, msg_r, (&sv).into())?;
        if self.cabinet.get_pre_vote_qc(msg_h, msg_r).is_some() {
            return Err(OverlordError::debug_old());
        }

        self.auth.verify_signed_pre_vote(&sv)?;
        if let Some(sum_w) = self.cabinet.insert(msg_h, msg_r, (&sv).into())? {
            info!(
                "[RECEIVE]\n\t<{}> <- {}\n\t<message> signed_pre_vote: {}\n\t<weight> cumulative_weight: {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
                self.address.tiny_hex(),
                sv.voter.tiny_hex(),
                sv,
                sum_w,
                self.state,
                self.prepare,
                self.sync,
            );
            if self.auth.current_auth.beyond_majority(sum_w.cum_weight) {
                let votes = self
                    .cabinet
                    .get_signed_pre_votes_by_hash(
                        msg_h,
                        sum_w.round,
                        &sum_w
                            .block_hash
                            .expect("Unreachable! Lost the vote_hash while beyond majority"),
                    )
                    .expect("Unreachable! Lost signed_pre_votes while beyond majority");
                let pre_vote_qc = self.auth.aggregate_pre_votes(votes)?;
                self.agent.broadcast(pre_vote_qc.clone().into()).await?;
            }
        }
        Ok(())
    }

    async fn handle_signed_pre_commit(&mut self, sv: SignedPreCommit) -> OverlordResult<()> {
        let msg_h = sv.vote.height;
        let msg_r = sv.vote.round;

        self.filter_msg(msg_h, msg_r, (&sv).into())?;
        if self.cabinet.get_pre_commit_qc(msg_h, msg_r).is_some() {
            return Err(OverlordError::debug_old());
        }

        self.auth.verify_signed_pre_commit(&sv)?;
        if let Some(sum_w) = self.cabinet.insert(msg_h, msg_r, (&sv).into())? {
            info!(
                "[RECEIVE]\n\t<{}> <- {}\n\t<message> signed_pre_commit: {}\n\t<weight> cumulative_weight: {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
                self.address.tiny_hex(),
                sv.voter.tiny_hex(),
                sv,
                sum_w,
                self.state,
                self.prepare,
                self.sync,
            );
            if self.auth.current_auth.beyond_majority(sum_w.cum_weight) {
                let votes = self
                    .cabinet
                    .get_signed_pre_commits_by_hash(
                        msg_h,
                        sum_w.round,
                        &sum_w
                            .block_hash
                            .expect("Unreachable! Lost the vote_hash while beyond majority"),
                    )
                    .expect("Unreachable! Lost signed_pre_votes while beyond majority");
                let pre_commit_qc = self.auth.aggregate_pre_commits(votes)?;
                self.agent.broadcast(pre_commit_qc.clone().into()).await?;
            }
        }
        Ok(())
    }

    async fn handle_signed_choke(&mut self, sc: SignedChoke) -> OverlordResult<()> {
        let msg_h = sc.choke.height;
        let msg_r = sc.choke.round;

        self.filter_msg(msg_h, msg_r, (&sc).into())?;
        if self.cabinet.get_choke_qc(msg_h, msg_r).is_some() {
            return Err(OverlordError::debug_old());
        }

        self.auth.verify_signed_choke(&sc)?;
        if let Some(sum_w) = self.cabinet.insert(msg_h, msg_r, (&sc).into())? {
            info!(
                "[RECEIVE]\n\t<{}> <- {}\n\t<message> signed_choke: {}\n\t<weight> cumulative_weight: {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
                self.address.tiny_hex(),
                sc.voter.tiny_hex(),
                sc,
                sum_w,
                self.state,
                self.prepare,
                self.sync
            );
            if self.auth.current_auth.beyond_majority(sum_w.cum_weight) {
                let chokes = self
                    .cabinet
                    .get_signed_chokes(msg_h, sum_w.round)
                    .expect("Unreachable! Lost signed_chokes while beyond majority");

                let choke_qc = self.auth.aggregate_chokes(chokes)?;
                self.handle_choke_qc(choke_qc).await?;
            } else if let Some(from) = sc.from {
                match from {
                    UpdateFrom::PreVoteQC(qc) => self.handle_pre_vote_qc(qc, true).await?,
                    UpdateFrom::PreCommitQC(qc) => self.handle_pre_commit_qc(qc, true).await?,
                    UpdateFrom::ChokeQC(qc) => self.handle_choke_qc(qc).await?,
                }
            }
        }
        Ok(())
    }

    async fn handle_pre_vote_qc(
        &mut self,
        qc: PreVoteQC,
        exist_uncertain: bool,
    ) -> OverlordResult<()> {
        let msg_h = qc.vote.height;
        let msg_r = qc.vote.round;

        self.filter_msg(msg_h, msg_r, (&qc).into())?;
        self.auth.verify_pre_vote_qc(&qc)?;
        if exist_uncertain {
            self.cabinet.insert(msg_h, msg_r, (&qc).into())?;
        }
        if self
            .cabinet
            .get_full_block(msg_h, &qc.vote.block_hash)
            .is_some()
        {
            let block = self
                .cabinet
                .get_block(msg_h, &qc.vote.block_hash)
                .expect("Unreachable! Lost a block which full block exist");
            let leader = self.auth.get_leader(msg_h, msg_r);
            let old_state = self.state.handle_pre_vote_qc(&qc, block.clone())?;
            self.log_state_update_of_msg(old_state, &leader, (&qc).into());

            self.agent.set_step_timeout(self.state.stage.clone());
            self.wal.save_state(&self.state)?;

            let vote = self.auth.sign_pre_commit(qc.vote.clone())?;
            self.agent.transmit(leader, vote.clone().into()).await?;
            // new design, when leader is down, next leader can aggregate votes to view change fast
            let next_leader = self.auth.get_leader(msg_h, msg_r + 1);
            self.agent.transmit(next_leader, vote.into()).await?;
        }
        Ok(())
    }

    async fn handle_pre_commit_qc(
        &mut self,
        qc: PreCommitQC,
        exist_uncertain: bool,
    ) -> OverlordResult<()> {
        let msg_h = qc.vote.height;
        let msg_r = qc.vote.round;

        self.filter_msg(msg_h, msg_r, (&qc).into())?;
        self.auth.verify_pre_commit_qc(&qc)?;
        if exist_uncertain {
            self.cabinet.insert(msg_h, msg_r, (&qc).into())?;
        }
        if self
            .cabinet
            .get_full_block(msg_h, &qc.vote.block_hash)
            .is_some()
        {
            let block = self
                .cabinet
                .get_block(msg_h, &qc.vote.block_hash)
                .expect("Unreachable! Lost a block which full block exist");
            let leader = self.auth.get_leader(msg_h, msg_r);
            let old_state = self.state.handle_pre_commit_qc(&qc, block.clone())?;
            self.log_state_update_of_msg(old_state, &leader, (&qc).into());
            self.wal.save_state(&self.state)?;

            let (commit_hash, proof, commit_exec_h) = self.save_and_exec_block().await;
            self.handle_commit(commit_hash, proof, commit_exec_h)
                .await?;
        }
        Ok(())
    }

    async fn handle_choke_qc(&mut self, qc: ChokeQC) -> OverlordResult<()> {
        let msg_h = qc.choke.height;
        let msg_r = qc.choke.round;

        self.filter_msg(msg_h, msg_r, (&qc).into())?;
        self.auth.verify_choke_qc(&qc)?;
        let old_state = self.state.handle_choke_qc(&qc)?;
        self.log_state_update_of_msg(old_state, &self.address, (&qc).into());
        self.wal.save_state(&self.state)?;
        info!(
            "[ROUND]\n\t<{}> -> new round {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n\n",
            self.address.tiny_hex(),
            self.state.stage.round,
            self.state,
            self.prepare,
            self.sync,
        );
        self.new_round().await
    }

    async fn save_and_exec_block(&mut self) -> (Hash, Proof, Height) {
        let proof = self
            .state
            .pre_commit_qc
            .as_ref()
            .expect("Unreachable! Lost pre_commit_qc when commit");
        let commit_hash = proof.vote.block_hash.clone();
        let height = self.state.stage.height;

        let full_block = self
            .cabinet
            .get_full_block(height, &commit_hash)
            .expect("Unreachable! Lost full block when commit");
        let request = ExecRequest::new(
            height,
            full_block.clone(),
            proof.clone(),
            self.prepare.last_exec_result.block_states.state.clone(),
            self.prepare
                .last_commit_exec_result
                .block_states
                .state
                .clone(),
        );
        self.agent.save_and_exec_block(request);

        let commit_exec_h = self
            .state
            .block
            .as_ref()
            .expect("Unreachable! Lost commit block when commit")
            .get_exec_height();

        (commit_hash, proof.clone(), commit_exec_h)
    }

    async fn handle_commit(
        &mut self,
        commit_hash: Hash,
        proof: Proof,
        commit_exec_h: Height,
    ) -> OverlordResult<()> {
        let commit_exec_result =
            self.prepare
                .handle_commit(commit_hash, proof.clone(), commit_exec_h);
        self.adapter
            .commit(Context::default(), commit_exec_result.clone())
            .await;

        let next_height = self.state.stage.height + 1;
        self.auth
            .handle_commit(commit_exec_result.consensus_config.auth_config.clone());
        self.cabinet.handle_commit(next_height, &self.auth);
        self.agent
            .handle_commit(commit_exec_result.consensus_config.time_config.clone());
        self.wal.handle_commit(next_height)?;

        info!(
            "[COMMIT]\n\t<{}> -> commit\n\t<config> {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n",
            self.address.tiny_hex(),
            commit_exec_result.consensus_config,
            self.state,
            self.prepare,
            self.sync,
        );

        // If self is leader, should not wait for interval timeout.
        // This is the exact opposite of the previous design.
        // So the time_config should also be changed.
        // make sure ( pre_vote_timeout >= propose_timeout + interval )
        if self.sync.state == SyncStat::Off
            && (!self.auth.am_i_leader(next_height, INIT_ROUND)
                || self.auth.current_auth.list.len() == 1)
            && self.agent.set_step_timeout(self.state.stage.clone())
        {
            return Ok(());
        }

        self.next_height().await
    }

    async fn handle_signed_height(&mut self, signed_height: SignedHeight) -> OverlordResult<()> {
        let my_height = self.state.stage.height;
        if signed_height.height <= my_height {
            return Err(OverlordError::debug_old());
        }
        if self.state.stage.step != Step::Brake {
            return Err(OverlordError::debug_not_ready(format!(
                "my.step != Step::Brake, {} != Step::Brake",
                self.state.stage.step
            )));
        }

        self.auth.verify_signed_height(&signed_height)?;
        let old_sync = self.sync.handle_signed_height(&signed_height)?;
        self.log_sync_update_of_msg(
            old_sync,
            &signed_height.address,
            signed_height.clone().into(),
        );
        self.agent.set_sync_timeout(self.sync.request_id);
        self.agent.set_clear_timeout(signed_height.address.clone());
        let range = HeightRange::new(my_height, BLOCK_BATCH);
        let request = self.auth.sign_sync_request(range)?;
        self.agent
            .transmit(signed_height.address.clone(), request.into())
            .await
    }

    async fn handle_sync_request(&mut self, request: SyncRequest) -> OverlordResult<()> {
        if request.request_range.from >= self.state.stage.height {
            return Err(OverlordError::byz_req_high(format!(
                "request_from > self.height, {} > {}",
                request.request_range.from, self.state.stage.height
            )));
        }

        self.auth.verify_sync_request(&request)?;
        let old_sync = self.sync.handle_sync_request(&request)?;
        self.log_sync_update_of_msg(old_sync, &request.requester, request.clone().into());
        self.agent.set_clear_timeout(request.requester.clone());
        let block_with_proofs = self
            .adapter
            .get_block_with_proofs(Context::default(), request.request_range.clone())
            .await
            .map_err(OverlordError::local_get_block)?;
        let response = self
            .auth
            .sign_sync_response(request.request_range.clone(), block_with_proofs)?;
        self.agent
            .transmit(request.requester, response.into())
            .await
    }

    async fn handle_sync_response(&mut self, response: SyncResponse<B>) -> OverlordResult<()> {
        if response.request_range.to < self.state.stage.height {
            return Err(OverlordError::debug_old());
        }
        self.auth.verify_sync_response(&response)?;

        let map: HashMap<Height, FullBlockWithProof<B>> = response
            .block_with_proofs
            .clone()
            .into_iter()
            .map(|fbp| (fbp.block.get_height(), fbp))
            .collect();

        info!(
            "[RECEIVE] \n\t<{}> <- {}\n\t<message> sync_response: {} \n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n",
            self.address.tiny_hex(),
            response.responder.tiny_hex(),
            response,
            self.state,
            self.prepare,
            self.sync,
        );

        while let Some(full_block_with_proof) = map.get(&self.state.stage.height).as_ref() {
            info!(
                "[SYNC] \n\t<{}> <- {}\n\t<message> full_block_with_proof: {} \n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n",
                self.address.tiny_hex(),
                response.responder.tiny_hex(),
                full_block_with_proof,
                self.state,
                self.prepare,
                self.sync,
            );

            let block = &full_block_with_proof.block;
            let proof = &full_block_with_proof.proof;
            let block_hash = block.get_block_hash().map_err(OverlordError::byz_hash)?;
            if proof.vote.block_hash != block_hash {
                return Err(OverlordError::byz_sync(format!(
                    "proof.vote_hash != block.hash, {} != {}",
                    proof.vote.block_hash.tiny_hex(),
                    block_hash.clone().tiny_hex()
                )));
            }
            self.check_block(block).await?;
            self.auth.verify_pre_commit_qc(proof)?;

            let full_block = full_block_with_proof.full_block.clone();
            let exec_result = self
                .adapter
                .save_and_exec_block_with_proof(
                    Context::default(),
                    block.get_height(),
                    full_block,
                    proof.clone(),
                    self.prepare.last_exec_result.block_states.state.clone(),
                    self.prepare
                        .last_commit_exec_result
                        .block_states
                        .state
                        .clone(),
                    true,
                )
                .await
                .expect("Execution is down! It's meaningless to continue running");
            self.prepare.handle_exec_result(exec_result);
            self.handle_commit(block_hash, proof.clone(), block.get_exec_height())
                .await?;
        }
        let old_sync = self.sync.handle_sync_response();
        self.log_sync_update_of_msg(old_sync, &response.responder.clone(), response.into());

        Ok(())
    }

    async fn handle_height_timeout(&mut self) -> OverlordResult<()> {
        self.agent.set_height_timeout();

        let height = self.state.stage.height;
        let signed_height = self.auth.sign_height(height)?;
        // Todo: can optimized by transmit a random peer instead of broadcast
        self.agent.broadcast(signed_height.into()).await
    }

    async fn next_height(&mut self) -> OverlordResult<()> {
        self.state.next_height();
        info!(
            "[HEIGHT]\n\t<{}> -> next height {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n\n",
            self.address.tiny_hex(),
            self.state.stage.height,
            self.state,
            self.prepare,
            self.sync,
        );
        self.wal.save_state(&self.state)?;
        self.agent.next_height();
        self.new_round().await
    }

    async fn new_round(&mut self) -> OverlordResult<()> {
        // if leader send proposal else search proposal, last set time
        let h = self.state.stage.height;
        let r = self.state.stage.round;

        self.agent.set_step_timeout(self.state.stage.clone());

        if self.auth.am_i_leader(h, r) {
            let signed_proposal = self.create_signed_proposal().await?;
            self.agent.broadcast(signed_proposal.into()).await?;
        } else if let Some(signed_proposal) = self.cabinet.take_signed_proposal(h, r) {
            self.handle_signed_proposal(signed_proposal).await?;
        }
        Ok(())
    }

    fn check_proposal(&self, p: &Proposal<B>) -> OverlordResult<()> {
        if p.height != p.block.get_height() {
            return Err(OverlordError::byz_block(format!(
                "proposal.height != block.height, {} != {}",
                p.height,
                p.block.get_height()
            )));
        }

        let block_hash = p.block.get_block_hash().map_err(OverlordError::byz_hash)?;
        if p.block_hash != block_hash {
            return Err(OverlordError::byz_block(format!(
                "proposal.block_hash != block.hash, {} != {}",
                p.block_hash.tiny_hex(),
                block_hash.tiny_hex()
            )));
        }

        if self.prepare.pre_hash != p.block.get_pre_hash() {
            return Err(OverlordError::byz_block(format!(
                "self.pre_hash != block.pre_hash, {} != {}",
                self.prepare.pre_hash.tiny_hex(),
                p.block.get_pre_hash().tiny_hex()
            )));
        }

        self.auth.verify_proof(p.block.get_proof())?;

        if let Some(lock) = &p.lock {
            if lock.vote.is_empty_vote() {
                return Err(OverlordError::byz_empty_lock());
            }
            self.auth.verify_pre_vote_qc(&lock)?;
        }
        Ok(())
    }

    async fn check_block(&self, block: &B) -> OverlordResult<()> {
        let exec_h = block.get_exec_height();
        if block.get_height() > exec_h + self.prepare.max_exec_behind {
            return Err(OverlordError::byz_block(format!(
                "block.block_height > block.exec_height + max_exec_behind, {} > {} + {}",
                block.get_height(),
                exec_h,
                self.prepare.max_exec_behind
            )));
        }
        if self.prepare.exec_height < exec_h {
            return Err(OverlordError::warn_block(format!(
                "self.exec_height < block.exec_height, {} < {}",
                self.prepare.exec_height, exec_h
            )));
        }

        self.adapter
            .check_block(
                Context::new(),
                block,
                &self.prepare.get_block_states_list(exec_h),
                &self.prepare.last_commit_exec_result.block_states.state,
            )
            .await
            .map_err(OverlordError::byz_adapter_check_block)?;
        Ok(())
    }

    async fn create_block(&self) -> OverlordResult<B> {
        let height = self.state.stage.height;
        let exec_height = self.prepare.exec_height;
        if height > exec_height + self.prepare.max_exec_behind {
            return Err(OverlordError::local_behind(format!(
                "self.height > self.exec_height + max_exec_behind, {} > {} + {}",
                height, exec_height, self.prepare.max_exec_behind
            )));
        }
        let pre_hash = self.prepare.pre_hash.clone();
        let pre_proof = self.prepare.pre_proof.clone();
        let block_states = self.prepare.get_block_states_list(exec_height);
        self.adapter
            .create_block(
                Context::default(),
                height,
                exec_height,
                pre_hash,
                pre_proof,
                block_states,
                self.prepare
                    .last_commit_exec_result
                    .block_states
                    .state
                    .clone(),
            )
            .await
            .map_err(OverlordError::local_create_block)
    }

    async fn create_signed_proposal(&self) -> OverlordResult<SignedProposal<B>> {
        let height = self.state.stage.height;
        let round = self.state.stage.round;
        let proposal = if let Some(lock) = &self.state.lock {
            let block = self
                .state
                .block
                .as_ref()
                .expect("Unreachable! Block is none when lock is some");
            let hash = lock.vote.block_hash.clone();
            Proposal::new(
                height,
                round,
                block.clone(),
                hash,
                Some(lock.clone()),
                self.address.clone(),
            )
        } else {
            let block = self.create_block().await?;
            let hash = block.get_block_hash().map_err(OverlordError::byz_hash)?;
            Proposal::new(height, round, block, hash, None, self.address.clone())
        };
        self.auth.sign_proposal(proposal)
    }

    fn filter_msg(
        &mut self,
        height: Height,
        round: Round,
        capsule: Capsule<B>,
    ) -> OverlordResult<()> {
        let my_height = self.state.stage.height;
        let my_round = self.state.stage.round;
        if height < my_height {
            return Err(OverlordError::debug_old());
        } else if height == my_height && round < my_round {
            return match capsule {
                Capsule::SignedProposal(_) => Ok(()),
                Capsule::PreCommitQC(qc) => {
                    if qc.vote.is_empty_vote() {
                        Err(OverlordError::debug_old())
                    } else {
                        Ok(())
                    }
                }
                _ => Err(OverlordError::debug_old()),
            };
        } else if height > my_height + HEIGHT_WINDOW || round > my_round + ROUND_WINDOW {
            return Err(OverlordError::net_much_high());
        } else if height > my_height {
            self.cabinet.insert(height, round, capsule.clone())?;
            return Err(OverlordError::debug_high());
        }
        Ok(())
    }

    fn extract_sender(&self, msg: &OverlordMsg<B>) -> Address {
        match msg {
            OverlordMsg::SignedProposal(sp) => sp.proposal.proposer.clone(),
            OverlordMsg::SignedPreVote(sv) => sv.voter.clone(),
            OverlordMsg::SignedPreCommit(sv) => sv.voter.clone(),
            OverlordMsg::SignedChoke(sc) => sc.voter.clone(),
            OverlordMsg::PreVoteQC(qc) => self.auth.get_leader(qc.vote.height, qc.vote.round),
            OverlordMsg::PreCommitQC(qc) => self.auth.get_leader(qc.vote.height, qc.vote.round),
            OverlordMsg::SignedHeight(sh) => sh.address.clone(),
            OverlordMsg::SyncRequest(sq) => sq.requester.clone(),
            OverlordMsg::SyncResponse(sp) => sp.responder.clone(),
            OverlordMsg::Stop => unreachable!(),
        }
    }

    async fn handle_err(&self, e: OverlordError, content: String) {
        match e.kind {
            ErrorKind::Debug => debug!("{}", content),
            ErrorKind::Warn => debug!("{}", content),
            ErrorKind::LocalError => error!("{}", content),
            _ => {
                error!("{}", content);
                self.adapter.handle_error(Context::default(), e).await;
            }
        }
    }

    fn log_sync_update_of_timeout(&self, old_sync: Sync<B>, timeout: TimeoutEvent) {
        info!(
            "[TIMEOUT]\n\t<{}> <- timeout\n\t<message> {}\n\t<state> {}\n\t<prepare> {} \n\t<sync> {} => {}\n",
            self.address.tiny_hex(),
            timeout,
            self.state,
            self.prepare,
            old_sync,
            self.sync,
        );
    }

    fn log_sync_update_of_msg(&self, old_sync: Sync<B>, from: &Address, msg: OverlordMsg<B>) {
        info!(
            "[RECEIVE]\n\t<{}> <- {}\n\t<message> {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {} => {}\n",
            self.address.tiny_hex(),
            from.tiny_hex(),
            msg,
            self.state,
            self.prepare,
            old_sync,
            self.sync
        );
    }

    fn log_state_update_of_timeout(&self, old_state: StateInfo<B>, timeout: TimeoutEvent) {
        info!(
            "[TIMEOUT]\n\t<{}> <- timeout\n\t<timeout> {} \n\t<state> {} => {} \n\t<prepare> {}\n\t<sync> {}\n\n",
            self.address.tiny_hex(),
            timeout,
            old_state,
            self.state,
            self.prepare,
            self.sync,
        );
    }

    fn log_state_update_of_msg(&self, old_state: StateInfo<B>, from: &Address, msg: Capsule<B>) {
        info!(
            "[RECEIVE] \n\t<{}> <- {}\n\t<message> {} \n\t<state> {} => {} \n\t<prepare> {}\n\t<sync> {}\n\n",
            self.address.tiny_hex(),
            from.tiny_hex(),
            msg,
            old_state,
            self.state,
            self.prepare,
            self.sync,
        );
    }
}

async fn recover_state_by_adapter<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
) -> StateInfo<B> {
    let height = adapter.get_latest_height(Context::default()).await.expect(
        "Cannot get the latest height from the adapter! It's meaningless to continue running",
    );
    StateInfo::from_commit_height(height)
}

async fn recover_propose_prepare_and_config<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
) -> ProposePrepare<S> {
    let last_commit_height = adapter.get_latest_height(Context::default()).await.expect(
        "Cannot get the latest height from the adapter! It's meaningless to continue running",
    );
    let full_block_with_proof = get_block_with_proof(adapter, last_commit_height)
        .await
        .unwrap()
        .unwrap();
    let block = full_block_with_proof.block;
    let proof = full_block_with_proof.proof;

    let hash = block.get_block_hash().expect("hash block failed");
    let last_exec_height = block.get_exec_height();
    let mut exec_results = vec![];
    let mut last_exec_result = ExecResult::default();
    let mut max_exec_behind = 5;
    for h in last_exec_height..=last_commit_height {
        let exec_result = get_exec_result(adapter, h).await;
        max_exec_behind = exec_result.consensus_config.max_exec_behind;
        if h == last_exec_height {
            last_exec_result = exec_result;
        } else {
            exec_results.push(exec_result);
        }
    }

    ProposePrepare::new(
        max_exec_behind,
        last_commit_height,
        last_exec_height,
        last_exec_result,
        exec_results,
        proof,
        hash,
    )
}

async fn get_block_with_proof<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
    height: Height,
) -> OverlordResult<Option<FullBlockWithProof<B>>> {
    let vec = adapter
        .get_block_with_proofs(Context::default(), HeightRange::new(height, 1))
        .await
        .map_err(OverlordError::local_get_block)?;
    Ok(Some(vec[0].clone()))
}

async fn get_exec_result<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
    height: Height,
) -> ExecResult<S> {
    adapter
        .get_block_exec_result(Context::default(), height)
        .await
        .unwrap_or_else(|_| panic!("Unreachable! Cannot get exec result of height {}", height))
}
