use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use bytes::Bytes;
use creep::Context;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::{select, StreamExt};
use log::{debug, error, info, warn};

use crate::error::ErrorKind;
use crate::state::{ProposePrepare, Stage, StateInfo};
use crate::types::{
    Choke, ChokeQC, CumWeight, FetchedFullBlock, FullBlockWithProof, PreCommitQC, PreVoteQC,
    Proposal, SignedChoke, SignedHeight, SignedPreCommit, SignedPreVote, SignedProposal,
    SyncRequest, SyncResponse, UpdateFrom,
};
use crate::utils::agent::{EventAgent, WrappedExecRequest, WrappedExecResult, WrappedOverlordMsg};
use crate::utils::auth::{AuthCell, AuthFixedConfig, AuthManage};
use crate::utils::cabinet::{Cabinet, Capsule};
use crate::utils::exec::ExecRequest;
use crate::utils::sync::{Sync, SyncStat, BLOCK_BATCH};
use crate::utils::timeout::TimeoutEvent;
use crate::{
    Adapter, Address, Blk, Crypto, ExecResult, Hash, Height, HeightRange, OverlordError,
    OverlordMsg, OverlordResult, Proof, Round, St, TinyHex, Wal, INIT_ROUND,
};

const HEIGHT_WINDOW: Height = 5;
const ROUND_WINDOW: Round = 5;

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
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        ctx: Context,
        auth_fixed_config: AuthFixedConfig,
        adapter: &Arc<A>,
        from_net: UnboundedReceiver<WrappedOverlordMsg<B>>,
        to_net: UnboundedSender<WrappedOverlordMsg<B>>,
        from_exec: UnboundedReceiver<WrappedExecResult<S>>,
        to_exec: UnboundedSender<WrappedExecRequest<S>>,
        wal_path: &str,
    ) -> Self {
        let address = &auth_fixed_config.address.clone();

        let wal = Wal::new(wal_path);
        let rst = wal.load_state();

        let state;
        let from;
        if let Err(e) = rst {
            warn!("Load state from wal failed: {}! Try to recover state by the adapter, which face security risk if majority auth nodes lost their wal file at the same time", e);
            state = recover_state_by_adapter(adapter, ctx.clone()).await;
            from = "adapter";
        } else {
            state = rst.unwrap();
            from = "wal"
        };

        let prepare = recover_propose_prepare_and_config(adapter, ctx.clone()).await;
        let last_exec_result = prepare.last_commit_exec_result.clone();
        let auth_config = last_exec_result.consensus_config.auth_config.clone();
        let time_config = last_exec_result.consensus_config.time_config.clone();
        let last_config = if prepare.exec_height > 0 {
            Some(
                get_exec_result(adapter, ctx.clone(), prepare.exec_height - 1)
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

    pub async fn run(mut self, ctx: Context) {
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone());
        self.agent.set_height_timeout(ctx);

        loop {
            select! {
                opt = self.agent.from_net.next() => {
                    let (ctx, msg) = opt.expect("Net Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.handle_msg(ctx.clone(), msg.clone()).await {
                        let sender = self.extract_sender(&msg);
                        let content = format!("[RECEIVE]\n\t<{}> <- {}\n\t<message> {}\n\t{}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
                            self.address.tiny_hex(), sender.tiny_hex(), msg, e, self.state, self.prepare, self.sync);
                        self.handle_err(ctx, e, content).await;
                    }
                }
                opt = self.agent.from_exec.next() => {
                    let (ctx, exec_result) = opt.expect("Exec Channel is down! It's meaningless to continue running");
                    self.handle_exec_result(ctx, exec_result);
                }
                opt = self.agent.from_fetch.next() => {
                    let (ctx, fetch) = opt.expect("Fetch Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.handle_fetch(ctx.clone(), fetch).await {
                        let content = format!("[FETCH]\n\t<{}> <- full block\n\t{}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n",
                            self.address.tiny_hex(), e, self.state, self.prepare, self.sync);
                        self.handle_err(ctx, e, content).await;
                    }
                }
                opt = self.agent.from_timeout.next() => {
                    let (ctx, timeout) = opt.expect("Timeout Channel is down! It's meaningless to continue running");
                    if let Err(e) = self.handle_timeout(ctx.clone(), timeout.clone()).await {
                        let content = format!("[TIMEOUT]\n\t<{}> <- timeout\n\t<timeout> {}\n\t{}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
                            self.address.tiny_hex(), timeout, e, self.state, self.prepare, self.sync);
                        self.handle_err(ctx, e, content).await;
                    }
                }
            }
        }
    }

    async fn handle_msg(&mut self, ctx: Context, msg: OverlordMsg<B>) -> OverlordResult<()> {
        match msg {
            OverlordMsg::SignedProposal(signed_proposal) => {
                self.handle_signed_proposal(ctx, signed_proposal).await?;
            }
            OverlordMsg::SignedPreVote(signed_pre_vote) => {
                self.handle_signed_pre_vote(ctx, signed_pre_vote).await?;
            }
            OverlordMsg::SignedPreCommit(signed_pre_commit) => {
                self.handle_signed_pre_commit(ctx, signed_pre_commit)
                    .await?;
            }
            OverlordMsg::SignedChoke(signed_choke) => {
                self.handle_signed_choke(ctx, signed_choke).await?;
            }
            OverlordMsg::PreVoteQC(pre_vote_qc) => {
                self.handle_pre_vote_qc(ctx, pre_vote_qc, true).await?;
            }
            OverlordMsg::PreCommitQC(pre_commit_qc) => {
                self.handle_pre_commit_qc(ctx, pre_commit_qc, true).await?;
            }
            OverlordMsg::SignedHeight(signed_height) => {
                self.handle_signed_height(ctx, signed_height).await?;
            }
            OverlordMsg::SyncRequest(request) => {
                self.handle_sync_request(ctx, request).await?;
            }
            OverlordMsg::SyncResponse(response) => {
                self.handle_sync_response(ctx, response).await?;
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

    fn handle_exec_result(&mut self, _ctx: Context, exec_result: ExecResult<S>) {
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
        ctx: Context,
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
            self.handle_pre_commit_qc(ctx, qc, false).await?;
        } else if let Some(qc) = self.cabinet.get_pre_vote_qc_by_hash(fetch.height, &hash) {
            self.handle_pre_vote_qc(ctx, qc, false).await?;
        }

        Ok(())
    }

    async fn handle_timeout(
        &mut self,
        ctx: Context,
        timeout_event: TimeoutEvent,
    ) -> OverlordResult<()> {
        match timeout_event {
            TimeoutEvent::ProposeTimeout(stage) => self.handle_propose_timeout(ctx, stage).await,
            TimeoutEvent::PreVoteTimeout(stage) => self.handle_pre_vote_timeout(ctx, stage).await,
            TimeoutEvent::PreCommitTimeout(stage) => {
                self.handle_pre_commit_timeout(ctx, stage).await
            }
            TimeoutEvent::BrakeTimeout(stage) => self.handle_brake_timeout(ctx, stage).await,
            TimeoutEvent::NextHeightTimeout => self.handle_next_height_timeout(ctx).await,
            TimeoutEvent::HeightTimeout => self.handle_height_timeout(ctx).await,
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

    async fn handle_propose_timeout(&mut self, ctx: Context, stage: Stage) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone());
        self.wal.save_state(&self.state)?;

        let signed_pre_vote = self.create_signed_pre_vote()?;
        self.transmit_vote(ctx, signed_pre_vote.into()).await
    }

    async fn handle_pre_vote_timeout(&mut self, ctx: Context, stage: Stage) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone());
        self.wal.save_state(&self.state)?;

        let signed_pre_commit = self.create_signed_pre_commit()?;
        self.transmit_vote(ctx, signed_pre_commit.into()).await
    }

    async fn handle_pre_commit_timeout(
        &mut self,
        ctx: Context,
        stage: Stage,
    ) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone());
        self.wal.save_state(&self.state)?;

        let signed_choke = self.create_signed_choke()?;
        self.agent.broadcast(ctx, signed_choke.into()).await
    }

    async fn handle_brake_timeout(&mut self, ctx: Context, stage: Stage) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone());

        let signed_choke = self.create_signed_choke()?;
        self.agent.broadcast(ctx, signed_choke.into()).await
    }

    async fn handle_next_height_timeout(&mut self, ctx: Context) -> OverlordResult<()> {
        self.next_height(ctx).await
    }

    async fn handle_signed_proposal(
        &mut self,
        ctx: Context,
        sp: SignedProposal<B>,
    ) -> OverlordResult<()> {
        let msg_h = sp.proposal.height;
        let msg_r = sp.proposal.round;

        self.filter_msg(msg_h, msg_r, (&sp).into())?;
        self.check_proposal(&sp.proposal)?;
        self.auth.verify_signed_proposal(&sp)?;
        self.cabinet.insert(msg_h, msg_r, (&sp).into())?;

        self.check_block(&sp.proposal.block).await?;
        self.agent
            .request_full_block(ctx.clone(), sp.proposal.block.clone());

        if msg_r > self.state.stage.round {
            if let Some(qc) = sp.proposal.lock {
                self.handle_pre_vote_qc(ctx.clone(), qc, true).await?;
            }
            return Err(OverlordError::debug_high());
        }

        let old_state = self.state.handle_signed_proposal(&sp)?;
        self.log_state_update_of_msg(old_state, &sp.proposal.proposer, (&sp).into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone());
        self.wal.save_state(&self.state)?;

        let vote = self.state.get_vote_for_proposal();
        let signed_pre_vote = self.auth.sign_pre_vote(vote)?;
        self.transmit_vote(ctx, signed_pre_vote.into()).await
    }

    async fn handle_signed_pre_vote(
        &mut self,
        ctx: Context,
        sv: SignedPreVote,
    ) -> OverlordResult<()> {
        let msg_h = sv.vote.height;
        let msg_r = sv.vote.round;

        self.filter_msg(msg_h, msg_r, (&sv).into())?;

        self.auth.verify_signed_pre_vote(&sv)?;
        self.cabinet.insert(msg_h, msg_r, (&sv).into())?;
        let max_w = self
            .cabinet
            .get_pre_vote_max_vote_weight(msg_h, msg_r)
            .expect("Unreachable! pre_vote_max_vote_weight has been set before");
        self.log_vote_received(&sv.voter, (&sv).into(), &max_w);

        self.try_aggregate_pre_votes(ctx, msg_h, msg_r, max_w).await
    }

    async fn handle_signed_pre_commit(
        &mut self,
        ctx: Context,
        sv: SignedPreCommit,
    ) -> OverlordResult<()> {
        let msg_h = sv.vote.height;
        let msg_r = sv.vote.round;

        self.filter_msg(msg_h, msg_r, (&sv).into())?;

        self.auth.verify_signed_pre_commit(&sv)?;
        self.cabinet.insert(msg_h, msg_r, (&sv).into())?;
        let max_w = self
            .cabinet
            .get_pre_commit_max_vote_weight(msg_h, msg_r)
            .expect("Unreachable! pre_commit_max_vote_weight has been set before");
        self.log_vote_received(&sv.voter, (&sv).into(), &max_w);

        self.try_aggregate_pre_commit(ctx, msg_h, msg_r, max_w)
            .await
    }

    async fn handle_signed_choke(&mut self, ctx: Context, sc: SignedChoke) -> OverlordResult<()> {
        let msg_h = sc.choke.height;
        let msg_r = sc.choke.round;

        self.filter_msg(msg_h, msg_r, (&sc).into())?;

        self.auth.verify_signed_choke(&sc)?;
        self.cabinet.insert(msg_h, msg_r, (&sc).into())?;
        let sum_w = self
            .cabinet
            .get_choke_vote_weight(msg_h, msg_r)
            .expect("Unreachable! choke_vote_weight has been set before");
        self.log_vote_received(&sc.voter, (&sc).into(), &sum_w);

        self.try_aggregate_choke(ctx.clone(), msg_h, msg_r, sum_w)
            .await?;
        if let Some(from) = sc.from {
            match from {
                UpdateFrom::PreVoteQC(qc) => self.handle_pre_vote_qc(ctx, qc, true).await?,
                UpdateFrom::PreCommitQC(qc) => self.handle_pre_commit_qc(ctx, qc, true).await?,
                UpdateFrom::ChokeQC(qc) => self.handle_choke_qc(ctx, qc).await?,
            }
        }
        Ok(())
    }

    async fn handle_pre_vote_qc(
        &mut self,
        ctx: Context,
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

        if qc.vote.is_empty_vote()
            || self
                .cabinet
                .get_full_block(msg_h, &qc.vote.block_hash)
                .is_some()
        {
            let block = self.cabinet.get_block(msg_h, &qc.vote.block_hash);
            let leader = self.auth.get_leader(msg_h, msg_r);
            let old_state = self.state.handle_pre_vote_qc(&qc, block)?;
            self.log_state_update_of_msg(old_state, &leader, (&qc).into());
            self.agent
                .set_step_timeout(ctx.clone(), self.state.stage.clone());
            self.wal.save_state(&self.state)?;

            let signed_pre_commit = self.create_signed_pre_commit()?;
            self.transmit_vote(ctx, signed_pre_commit.into()).await
        } else {
            Err(OverlordError::warn_wait())
        }
    }

    async fn handle_pre_commit_qc(
        &mut self,
        ctx: Context,
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

        if qc.vote.is_empty_vote()
            || self
                .cabinet
                .get_full_block(msg_h, &qc.vote.block_hash)
                .is_some()
        {
            let block = self.cabinet.get_block(msg_h, &qc.vote.block_hash);
            let leader = self.auth.get_leader(msg_h, msg_r);
            let old_state = self.state.handle_pre_commit_qc(&qc, block)?;
            self.log_state_update_of_msg(old_state, &leader, (&qc).into());
            self.wal.save_state(&self.state)?;

            if qc.vote.is_empty_vote() {
                self.new_round(ctx.clone()).await
            } else {
                let (commit_hash, proof, commit_exec_h) =
                    self.save_and_exec_block(ctx.clone()).await;
                self.handle_commit(ctx, commit_hash, proof, commit_exec_h)
                    .await
            }
        } else {
            Err(OverlordError::warn_wait())
        }
    }

    async fn handle_choke_qc(&mut self, ctx: Context, qc: ChokeQC) -> OverlordResult<()> {
        let msg_h = qc.choke.height;
        let msg_r = qc.choke.round;

        self.filter_msg(msg_h, msg_r, (&qc).into())?;
        self.auth.verify_choke_qc(&qc)?;
        let old_state = self.state.handle_choke_qc(&qc)?;
        self.log_state_update_of_msg(old_state, &self.address, (&qc).into());
        self.wal.save_state(&self.state)?;
        self.new_round(ctx).await
    }

    async fn save_and_exec_block(&mut self, ctx: Context) -> (Hash, Proof, Height) {
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
            self.prepare.last_exec_result.block_states.state.clone(),
            self.prepare
                .last_commit_exec_result
                .block_states
                .state
                .clone(),
        );
        self.adapter
            .save_full_block_with_proof(
                ctx.clone(),
                height,
                full_block.clone(),
                proof.clone(),
                false,
            )
            .await
            .expect("Unreachable! Commit block with proof must store success");
        self.agent.exec_block(ctx, request);

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
        ctx: Context,
        commit_hash: Hash,
        proof: Proof,
        commit_exec_h: Height,
    ) -> OverlordResult<()> {
        let commit_exec_result =
            self.prepare
                .handle_commit(commit_hash, proof.clone(), commit_exec_h);
        self.adapter
            .commit(ctx.clone(), commit_exec_result.clone())
            .await;

        let next_height = self.state.stage.height + 1;
        self.auth
            .handle_commit(commit_exec_result.consensus_config.auth_config.clone());
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

        if self.sync.state == SyncStat::Off
            && self.next_height_leader() == self.address
            && self
                .agent
                .set_step_timeout(ctx.clone(), self.state.stage.clone())
        {
            return Ok(());
        }

        self.next_height(ctx).await
    }

    async fn handle_signed_height(
        &mut self,
        ctx: Context,
        signed_height: SignedHeight,
    ) -> OverlordResult<()> {
        let my_height = self.state.stage.height;
        if signed_height.height <= my_height {
            return Err(OverlordError::debug_old());
        }
        // if self.state.stage.step != Step::Brake {
        //     return Err(OverlordError::debug_not_ready(format!(
        //         "my.step != Step::Brake, {} != Step::Brake",
        //         self.state.stage.step
        //     )));
        // }

        self.auth.verify_signed_height(&signed_height)?;
        let old_sync = self.sync.handle_signed_height(&signed_height)?;
        self.log_sync_update_of_msg(
            old_sync,
            &signed_height.address,
            signed_height.clone().into(),
        );
        self.agent
            .set_sync_timeout(ctx.clone(), self.sync.request_id);
        self.agent
            .set_clear_timeout(ctx.clone(), signed_height.address.clone());
        let range = HeightRange::new(my_height, BLOCK_BATCH);
        let request = self.auth.sign_sync_request(range)?;
        self.agent
            .transmit(ctx, signed_height.address.clone(), request.into())
            .await
    }

    async fn handle_sync_request(
        &mut self,
        ctx: Context,
        request: SyncRequest,
    ) -> OverlordResult<()> {
        if request.request_range.from >= self.state.stage.height {
            return Err(OverlordError::byz_req_high(format!(
                "request_from > self.height, {} > {}",
                request.request_range.from, self.state.stage.height
            )));
        }

        self.auth.verify_sync_request(&request)?;
        let old_sync = self.sync.handle_sync_request(&request)?;
        self.log_sync_update_of_msg(old_sync, &request.requester, request.clone().into());
        self.agent
            .set_clear_timeout(ctx.clone(), request.requester.clone());

        let adapter = Arc::clone(&self.adapter);
        let self_address = self.address.clone();
        let pri_key = self.auth.fixed_config.pri_key.clone();
        let pub_key = self.auth.fixed_config.pub_key.clone();

        tokio::spawn(async move {
            let block_with_proofs = adapter
                .get_full_block_with_proofs(ctx.clone(), request.request_range.clone())
                .await
                .map_err(OverlordError::local_get_block)?;

            let hash = A::CryptoImpl::hash(&Bytes::from(rlp::encode(&request.request_range)));
            let signature =
                A::CryptoImpl::sign_msg(pri_key, &hash).map_err(OverlordError::local_crypto)?;
            let response = SyncResponse::new(
                request.request_range.clone(),
                block_with_proofs,
                self_address.clone(),
                pub_key,
                signature,
            );
            info!(
                "[TRANSMIT]\n\t<{}> -> {}\n\t<message> sync_response: {} \n\n\n\n\n",
                self_address.tiny_hex(),
                request.requester.tiny_hex(),
                response
            );
            adapter
                .transmit(ctx, request.requester, response.into())
                .await
                .map_err(OverlordError::net_transmit)
        });
        Ok(())
    }

    async fn handle_sync_response(
        &mut self,
        ctx: Context,
        response: SyncResponse<B>,
    ) -> OverlordResult<()> {
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

            let sync_height = self.state.stage.height;
            let prepare_height = self.prepare.last_exec_result.block_states.height;
            if prepare_height != sync_height - 1 {
                self.sync.turn_off_sync();
                return Err(OverlordError::warn_not_ready(format!(
                    "prepare_height != sync_height - 1, {} != {} - 1",
                    prepare_height, sync_height
                )));
            }

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
            self.adapter
                .save_full_block_with_proof(
                    ctx.clone(),
                    sync_height,
                    full_block.clone(),
                    proof.clone(),
                    true,
                )
                .await
                .map_err(OverlordError::byz_save_block)?;
            let exec_result = self
                .adapter
                .exec_full_block(
                    ctx.clone(),
                    block.get_height(),
                    full_block,
                    self.prepare.last_exec_result.block_states.state.clone(),
                    self.prepare
                        .last_commit_exec_result
                        .block_states
                        .state
                        .clone(),
                )
                .await
                .map_err(OverlordError::byz_exec_block)?;
            self.prepare.handle_exec_result(exec_result);
            self.handle_commit(
                ctx.clone(),
                block_hash,
                proof.clone(),
                block.get_exec_height(),
            )
            .await?;
        }
        let old_sync = self.sync.handle_sync_response();
        self.log_sync_update_of_msg(old_sync, &response.responder.clone(), response.into());

        Ok(())
    }

    async fn handle_height_timeout(&mut self, ctx: Context) -> OverlordResult<()> {
        self.agent.set_height_timeout(ctx.clone());

        let height = self.state.stage.height;
        let signed_height = self.auth.sign_height(height)?;
        // Todo: can optimized by transmit a random peer instead of broadcast
        self.agent.broadcast(ctx, signed_height.into()).await
    }

    async fn next_height(&mut self, ctx: Context) -> OverlordResult<()> {
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
        self.replay_msg_received(ctx.clone());
        self.cabinet.next_height(self.state.stage.height);
        self.new_round(ctx).await
    }

    async fn new_round(&mut self, ctx: Context) -> OverlordResult<()> {
        info!(
            "[ROUND]\n\t<{}> -> new round {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n\n\n",
            self.address.tiny_hex(),
            self.state.stage.round,
            self.state,
            self.prepare,
            self.sync,
        );
        // if leader send proposal else search proposal, last set time
        let h = self.state.stage.height;
        let r = self.state.stage.round;

        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone());

        if self.is_current_leader() {
            let signed_proposal = self.create_signed_proposal(ctx.clone()).await?;
            self.agent.broadcast(ctx, signed_proposal.into()).await?;
        } else if let Some(signed_proposal) = self.cabinet.take_signed_proposal(h, r) {
            self.handle_signed_proposal(ctx, signed_proposal).await?;
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

    async fn create_block(&self, ctx: Context) -> OverlordResult<B> {
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
                ctx,
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

    async fn create_signed_proposal(&self, ctx: Context) -> OverlordResult<SignedProposal<B>> {
        let height = self.state.stage.height;
        let round = self.state.stage.round;
        let lock = &self.state.lock;
        let proposal = if lock.is_some() && !lock.as_ref().unwrap().vote.is_empty_vote() {
            let block = self
                .state
                .block
                .as_ref()
                .expect("Unreachable! Block is none when lock is some");
            let hash = lock.as_ref().unwrap().vote.block_hash.clone();
            Proposal::new(
                height,
                round,
                block.clone(),
                hash,
                lock.clone(),
                self.address.clone(),
            )
        } else {
            let block = self.create_block(ctx).await?;
            let hash = block.get_block_hash().map_err(OverlordError::byz_hash)?;
            Proposal::new(height, round, block, hash, None, self.address.clone())
        };
        self.auth.sign_proposal(proposal)
    }

    fn create_signed_pre_vote(&self) -> OverlordResult<SignedPreVote> {
        let vote = self.state.get_vote();
        self.auth.sign_pre_vote(vote)
    }

    fn create_signed_pre_commit(&self) -> OverlordResult<SignedPreCommit> {
        let vote = self.state.get_vote();
        self.auth.sign_pre_commit(vote)
    }

    fn create_signed_choke(&self) -> OverlordResult<SignedChoke> {
        let height = self.state.stage.height;
        let round = self.state.stage.round;
        let from = self.state.from.clone();
        let choke = Choke::new(height, round);
        self.auth.sign_choke(choke, from)
    }

    fn replay_msg_received(&mut self, ctx: Context) {
        if self.sync.state == SyncStat::Off {
            if let Some(grids) = self.cabinet.pop(self.state.stage.height) {
                for mut grid in grids {
                    for sc in grid.get_signed_chokes() {
                        self.agent.send_to_myself(ctx.clone(), sc.into());
                    }
                    if let Some(qc) = grid.get_pre_commit_qc() {
                        self.agent.send_to_myself(ctx.clone(), qc.into());
                    }
                    if let Some(qc) = grid.get_pre_vote_qc() {
                        self.agent.send_to_myself(ctx.clone(), qc.into());
                    }
                    for sv in grid.get_signed_pre_commits() {
                        self.agent.send_to_myself(ctx.clone(), sv.into());
                    }
                    for sv in grid.get_signed_pre_votes() {
                        self.agent.send_to_myself(ctx.clone(), sv.into());
                    }
                    if let Some(sp) = grid.take_signed_proposal() {
                        self.agent.send_to_myself(ctx.clone(), sp.into());
                    }
                }
            }
        }
    }

    async fn try_aggregate_pre_votes(
        &mut self,
        ctx: Context,
        height: Height,
        round: Round,
        max_w: CumWeight,
    ) -> OverlordResult<()> {
        if self.auth.current_auth.beyond_majority(max_w.cum_weight) {
            let votes = self
                .cabinet
                .get_signed_pre_votes_by_hash(
                    height,
                    round,
                    &max_w
                        .block_hash
                        .expect("Unreachable! Lost the vote_hash while beyond majority"),
                )
                .expect("Unreachable! Lost signed_pre_votes while beyond majority");
            let pre_vote_qc = self.auth.aggregate_pre_votes(votes)?;
            self.agent
                .broadcast(ctx, pre_vote_qc.clone().into())
                .await?;
        }
        Ok(())
    }

    async fn try_aggregate_pre_commit(
        &mut self,
        ctx: Context,
        height: Height,
        round: Round,
        max_w: CumWeight,
    ) -> OverlordResult<()> {
        if self.auth.current_auth.beyond_majority(max_w.cum_weight) {
            let votes = self
                .cabinet
                .get_signed_pre_commits_by_hash(
                    height,
                    round,
                    &max_w
                        .block_hash
                        .expect("Unreachable! Lost the vote_hash while beyond majority"),
                )
                .expect("Unreachable! Lost signed_pre_commits while beyond majority");
            let pre_commit_qc = self.auth.aggregate_pre_commits(votes)?;
            self.agent
                .broadcast(ctx, pre_commit_qc.clone().into())
                .await?;
        }
        Ok(())
    }

    async fn try_aggregate_choke(
        &mut self,
        ctx: Context,
        height: Height,
        round: Round,
        max_w: CumWeight,
    ) -> OverlordResult<()> {
        if self.auth.current_auth.beyond_majority(max_w.cum_weight) {
            let chokes = self
                .cabinet
                .get_signed_chokes(height, round)
                .expect("Unreachable! Lost signed_chokes while beyond majority");

            let choke_qc = self.auth.aggregate_chokes(chokes)?;
            self.handle_choke_qc(ctx, choke_qc).await?;
        }
        Ok(())
    }

    async fn transmit_vote(&self, ctx: Context, vote: OverlordMsg<B>) -> OverlordResult<()> {
        let leader = self.current_leader();
        self.agent
            .transmit(ctx.clone(), leader, vote.clone())
            .await?;
        // new design, when leader is down, next leader can aggregate votes to view change fast
        let next_leader = self.next_round_leader();
        self.agent.transmit(ctx, next_leader, vote).await
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
                // allow signed_proposal which round < self.round can be pass to fetch full block
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
            return Err(OverlordError::debug_high());
        } else if height > my_height {
            self.cabinet.insert(height, round, capsule.clone())?;
            return Err(OverlordError::debug_high());
        } else {
            match capsule {
                Capsule::SignedPreVote(sv) => {
                    if self
                        .cabinet
                        .get_pre_vote_qc(sv.vote.height, sv.vote.round)
                        .is_some()
                    {
                        return Err(OverlordError::debug_old());
                    }
                }
                Capsule::SignedPreCommit(sv) => {
                    if self
                        .cabinet
                        .get_pre_commit_qc(sv.vote.height, sv.vote.round)
                        .is_some()
                    {
                        return Err(OverlordError::debug_old());
                    }
                }
                Capsule::SignedChoke(sc) => {
                    if self
                        .cabinet
                        .get_choke_qc(sc.choke.height, sc.choke.round)
                        .is_some()
                    {
                        return Err(OverlordError::debug_old());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn current_leader(&self) -> Address {
        self.auth
            .get_leader(self.state.stage.height, self.state.stage.round)
    }

    fn next_round_leader(&self) -> Address {
        self.auth
            .get_leader(self.state.stage.height, self.state.stage.round + 1)
    }

    fn next_height_leader(&self) -> Address {
        self.auth
            .get_leader(self.state.stage.height + 1, INIT_ROUND)
    }

    fn is_current_leader(&self) -> bool {
        self.address == self.current_leader()
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

    async fn handle_err(&self, ctx: Context, e: OverlordError, content: String) {
        match e.kind {
            ErrorKind::Debug => debug!("{}", content),
            ErrorKind::Warn => debug!("{}", content),
            ErrorKind::LocalError => error!("{}", content),
            _ => {
                error!("{}", content);
                self.adapter.handle_error(ctx, e).await;
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

    fn log_vote_received(&self, from: &Address, msg: Capsule<B>, sum_w: &CumWeight) {
        info!(
            "[RECEIVE]\n\t<{}> <- {}\n\t<message> {}\n\t<weight> cumulative_weight: {}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
            self.address.tiny_hex(),
            from.tiny_hex(),
            msg,
            sum_w,
            self.state,
            self.prepare,
            self.sync,
        );
    }
}

async fn recover_state_by_adapter<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
    ctx: Context,
) -> StateInfo<B> {
    let height = adapter.get_latest_height(ctx).await.expect(
        "Cannot get the latest height from the adapter! It's meaningless to continue running",
    );
    StateInfo::from_commit_height(height)
}

async fn recover_propose_prepare_and_config<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
    ctx: Context,
) -> ProposePrepare<S> {
    let last_commit_height = adapter.get_latest_height(ctx.clone()).await.expect(
        "Cannot get the latest height from the adapter! It's meaningless to continue running",
    );
    let full_block_with_proof =
        get_full_block_with_proofs(adapter, ctx.clone(), last_commit_height)
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
        let exec_result = get_exec_result(adapter, ctx.clone(), h).await;
        max_exec_behind = exec_result.consensus_config.max_exec_behind;
        last_exec_result = exec_result.clone();
        if h > last_exec_height {
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

async fn get_full_block_with_proofs<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
    ctx: Context,
    height: Height,
) -> OverlordResult<Option<FullBlockWithProof<B>>> {
    let vec = adapter
        .get_full_block_with_proofs(ctx, HeightRange::new(height, 1))
        .await
        .map_err(OverlordError::local_get_block)?;
    Ok(Some(vec[0].clone()))
}

async fn get_exec_result<A: Adapter<B, S>, B: Blk, S: St>(
    adapter: &Arc<A>,
    ctx: Context,
    height: Height,
) -> ExecResult<S> {
    adapter
        .get_block_exec_result(ctx, height)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "Unreachable! Cannot get exec result of height {}, {}",
                height, e
            )
        })
}
