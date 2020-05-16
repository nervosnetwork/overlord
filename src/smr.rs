use std::collections::HashMap;
use std::sync::Arc;

use creep::Context;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use log::{debug, error, info, warn};

use crate::error::ErrorKind;
use crate::state::{ProposePrepare, Stage, StateInfo, Step};
use crate::types::{
    Choke, ChokeQC, CumWeight, FetchedFullBlock, FullBlockWithProof, PreCommitQC, PreVoteQC,
    Proposal, SignedChoke, SignedHeight, SignedPreCommit, SignedPreVote, SignedProposal,
    SyncRequest, SyncResponse, UpdateFrom, Vote,
};
use crate::utils::agent::{ChannelMsg, EventAgent};
use crate::utils::auth::{AuthCell, AuthManage};
use crate::utils::cabinet::{Cabinet, Capsule};
use crate::utils::exec::ExecRequest;
use crate::utils::sync::{Sync, SyncStat, BLOCK_BATCH};
use crate::utils::timeout::TimeoutEvent;
use crate::{
    Adapter, Address, Blk, CryptoConfig, ExecResult, FullBlk, Hash, Height, HeightRange,
    OverlordError, OverlordMsg, OverlordResult, Proof, Round, St, TinyHex, Wal, INIT_ROUND,
};

const HEIGHT_WINDOW: Height = 5;
const ROUND_WINDOW: Round = 5;

/// State Machine Replica
pub struct SMR<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St> {
    address: Address,
    state:   StateInfo<B>,
    prepare: ProposePrepare<S>,
    sync:    Sync<B>,

    wal:     Wal<B, F>,
    cabinet: Cabinet<B, F>,

    adapter: Arc<A>,
    auth:    Arc<AuthManage<A, B, F, S>>,
    agent:   Arc<EventAgent<A, B, F, S>>,
}

impl<A, B, F, S> SMR<A, B, F, S>
where
    A: Adapter<B, F, S>,
    B: Blk,
    F: FullBlk<B>,
    S: St,
{
    pub async fn new(
        ctx: Context,
        auth_crypto_config: CryptoConfig,
        adapter: &Arc<A>,
        smr_receiver: UnboundedReceiver<ChannelMsg<B, F, S>>,
        smr_sender: UnboundedSender<ChannelMsg<B, F, S>>,
        exec_sender: UnboundedSender<ChannelMsg<B, F, S>>,
        wal_path: &str,
    ) -> Self {
        let address = &auth_crypto_config.address.clone();

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
            address: address.clone(),
            sync: Sync::new(),
            adapter: Arc::<A>::clone(adapter),
            auth: Arc::new(AuthManage::new(auth_crypto_config, current_auth, last_auth)),
            agent: Arc::new(EventAgent::new(
                address.clone(),
                adapter,
                time_config,
                smr_receiver,
                smr_sender,
                exec_sender,
            )),
        }
    }

    pub async fn run(mut self, ctx: Context) {
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone())
            .await;
        self.agent.set_height_timeout(ctx).await;

        loop {
            let channel_msg = self.agent.next_channel_msg().await;
            match channel_msg {
                ChannelMsg::PreHandleMsg(ctx, msg) => {
                    if let Err(e) = self.pre_handle_msg(ctx.clone(), msg.clone()).await {
                        self.handle_msg_err(ctx, msg, e).await;
                    }
                }
                ChannelMsg::HandleMsg(ctx, msg) => {
                    if let Err(e) = self.handle_msg(ctx.clone(), msg.clone()).await {
                        self.handle_msg_err(ctx, msg, e).await;
                    }
                }
                ChannelMsg::HandleMsgError(ctx, msg, e) => {
                    self.handle_msg_err(ctx, msg, e).await;
                }
                ChannelMsg::HandleTimeoutError(ctx, timeout, e) => {
                    self.handle_timeout_err(ctx, timeout, e).await;
                }
                ChannelMsg::ExecResult(ctx, exec_result) => {
                    self.handle_exec_result(ctx, exec_result);
                }
                ChannelMsg::TimeoutEvent(ctx, timeout) => {
                    if let Err(e) = self.handle_timeout(ctx.clone(), timeout.clone()).await {
                        self.handle_timeout_err(ctx, timeout, e).await;
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    async fn pre_handle_msg(&mut self, ctx: Context, msg: OverlordMsg<B, F>) -> OverlordResult<()> {
        match msg {
            OverlordMsg::SignedProposal(signed_proposal) => {
                self.pre_handle_signed_proposal(ctx, signed_proposal)
                    .await?;
            }
            OverlordMsg::SignedPreVote(signed_pre_vote) => {
                self.pre_handle_signed_pre_vote(ctx, signed_pre_vote)
                    .await?;
            }
            OverlordMsg::SignedPreCommit(signed_pre_commit) => {
                self.pre_handle_signed_pre_commit(ctx, signed_pre_commit)
                    .await?;
            }
            OverlordMsg::SignedChoke(signed_choke) => {
                self.pre_handle_signed_choke(ctx, signed_choke).await?;
            }
            OverlordMsg::PreVoteQC(pre_vote_qc) => {
                self.pre_handle_pre_vote_qc(ctx, pre_vote_qc).await?;
            }
            OverlordMsg::PreCommitQC(pre_commit_qc) => {
                self.pre_handle_pre_commit_qc(ctx, pre_commit_qc).await?;
            }
            OverlordMsg::SignedHeight(signed_height) => {
                self.pre_handle_signed_height(ctx, signed_height).await?;
            }
            OverlordMsg::SyncRequest(request) => {
                self.pre_handle_sync_request(ctx, request).await?;
            }
            OverlordMsg::SyncResponse(response) => {
                self.pre_handle_sync_response(ctx, response).await?;
            }
            OverlordMsg::FetchedFullBlock(fetched_full_block) => {
                self.pre_handle_fetched_full_block(ctx, fetched_full_block)
                    .await?;
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
            _ => unreachable!(),
        }

        Ok(())
    }

    async fn handle_msg(&mut self, ctx: Context, msg: OverlordMsg<B, F>) -> OverlordResult<()> {
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
                self.handle_pre_vote_qc(ctx, pre_vote_qc).await?;
            }
            OverlordMsg::PreCommitQC(pre_commit_qc) => {
                self.handle_pre_commit_qc(ctx, pre_commit_qc).await?;
            }
            OverlordMsg::ChokeQC(choke_qc) => {
                self.handle_choke_qc(ctx, choke_qc).await?;
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
            OverlordMsg::FetchedFullBlock(fetched_full_block) => {
                self.handle_fetched_full_block(ctx, fetched_full_block)
                    .await?;
            }
            _ => unreachable!(),
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
        self.log_state_update_of_timeout(old_state, stage.clone().into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone())
            .await;
        self.wal.save_state(&self.state)?;

        let vote = self.state.get_vote();
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        let current_height = self.state.stage.height;
        let current_round = self.state.stage.round;
        tokio::spawn(async move {
            if let Err(e) = create_and_transmit_signed_pre_vote(
                &auth,
                &agent,
                current_height,
                current_round,
                ctx.clone(),
                vote,
            )
            .await
            {
                agent.send_to_myself(ChannelMsg::HandleTimeoutError(
                    ctx,
                    TimeoutEvent::from(stage),
                    e,
                ));
            }
        });
        Ok(())
    }

    async fn handle_pre_vote_timeout(&mut self, ctx: Context, stage: Stage) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.clone().into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone())
            .await;
        self.wal.save_state(&self.state)?;

        let vote = self.state.get_vote();
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        let current_height = self.state.stage.height;
        let current_round = self.state.stage.round;
        tokio::spawn(async move {
            if let Err(e) = create_and_transmit_signed_pre_commit(
                &auth,
                &agent,
                current_height,
                current_round,
                ctx.clone(),
                vote,
            )
            .await
            {
                agent.send_to_myself(ChannelMsg::HandleTimeoutError(
                    ctx,
                    TimeoutEvent::from(stage),
                    e,
                ));
            }
        });
        Ok(())
    }

    async fn handle_pre_commit_timeout(
        &mut self,
        ctx: Context,
        stage: Stage,
    ) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.clone().into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone())
            .await;
        self.wal.save_state(&self.state)?;

        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        let height = self.state.stage.height;
        let round = self.state.stage.round;
        let from = self.state.from.clone();
        let choke = Choke::new(height, round);
        tokio::spawn(async move {
            if let Err(e) =
                create_and_broadcast_signed_choke(&auth, &agent, ctx.clone(), choke, from).await
            {
                agent.send_to_myself(ChannelMsg::HandleTimeoutError(
                    ctx,
                    TimeoutEvent::from(stage),
                    e,
                ));
            }
        });
        Ok(())
    }

    async fn handle_brake_timeout(&mut self, ctx: Context, stage: Stage) -> OverlordResult<()> {
        let old_state = self.state.handle_timeout(&stage)?;
        self.log_state_update_of_timeout(old_state, stage.clone().into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone())
            .await;

        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        let height = self.state.stage.height;
        let round = self.state.stage.round;
        let from = self.state.from.clone();
        let choke = Choke::new(height, round);
        tokio::spawn(async move {
            if let Err(e) =
                create_and_broadcast_signed_choke(&auth, &agent, ctx.clone(), choke, from).await
            {
                agent.send_to_myself(ChannelMsg::HandleTimeoutError(
                    ctx,
                    TimeoutEvent::from(stage),
                    e,
                ));
            }
        });
        Ok(())
    }

    async fn handle_next_height_timeout(&mut self, ctx: Context) -> OverlordResult<()> {
        self.next_height(ctx).await
    }

    async fn pre_handle_signed_proposal(
        &mut self,
        ctx: Context,
        sp: SignedProposal<B>,
    ) -> OverlordResult<()> {
        self.filter_msg(
            sp.proposal.height,
            sp.proposal.round,
            ctx.clone(),
            (&sp).into(),
        )?;

        let prepare = self.prepare.clone();
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        let adapter = Arc::<_>::clone(&self.adapter);
        let state_round = self.state.stage.round;
        tokio::spawn(async move {
            let rst = pre_handle_signed_proposal(
                state_round,
                prepare,
                auth,
                &agent,
                adapter,
                ctx.clone(),
                sp.clone(),
            )
            .await;
            if let Err(e) = rst {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, sp.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, sp.into()));
            }
        });

        Ok(())
    }

    async fn handle_signed_proposal(
        &mut self,
        ctx: Context,
        sp: SignedProposal<B>,
    ) -> OverlordResult<()> {
        self.cabinet.insert(
            sp.proposal.height,
            sp.proposal.round,
            ctx.clone(),
            (&sp).into(),
        )?;

        let old_state = self.state.handle_signed_proposal(&sp)?;
        self.log_state_update_of_msg(old_state, &sp.proposal.proposer, (&sp).into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone())
            .await;
        self.wal.save_state(&self.state)?;

        let vote = self.state.get_vote_for_proposal();
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        let current_height = self.state.stage.height;
        let current_round = self.state.stage.round;
        tokio::spawn(async move {
            if let Err(e) = create_and_transmit_signed_pre_vote(
                &auth,
                &agent,
                current_height,
                current_round,
                ctx.clone(),
                vote,
            )
            .await
            {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, sp.into(), e));
            }
        });
        Ok(())
    }

    async fn pre_handle_signed_pre_vote(
        &mut self,
        ctx: Context,
        sv: SignedPreVote,
    ) -> OverlordResult<()> {
        self.filter_msg(sv.vote.height, sv.vote.round, ctx.clone(), (&sv).into())?;
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = auth.verify_signed_pre_vote(&sv).await {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, sv.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, sv.into()));
            }
        });
        Ok(())
    }

    async fn handle_signed_pre_vote(
        &mut self,
        ctx: Context,
        sv: SignedPreVote,
    ) -> OverlordResult<()> {
        let msg_h = sv.vote.height;
        let msg_r = sv.vote.round;
        self.cabinet
            .insert(msg_h, msg_r, ctx.clone(), (&sv).into())?;
        let max_w = self
            .cabinet
            .get_pre_vote_max_vote_weight(msg_h, msg_r)
            .expect("Unreachable! pre_vote_max_vote_weight has been set before");
        self.log_vote_received(&sv.voter, (&sv).into(), &max_w);

        self.try_aggregate_pre_votes(ctx, msg_h, msg_r, max_w, sv)
            .await
    }

    async fn pre_handle_signed_pre_commit(
        &mut self,
        ctx: Context,
        sv: SignedPreCommit,
    ) -> OverlordResult<()> {
        self.filter_msg(sv.vote.height, sv.vote.round, ctx.clone(), (&sv).into())?;
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = auth.verify_signed_pre_commit(&sv).await {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, sv.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, sv.into()));
            }
        });
        Ok(())
    }

    async fn handle_signed_pre_commit(
        &mut self,
        ctx: Context,
        sv: SignedPreCommit,
    ) -> OverlordResult<()> {
        let msg_h = sv.vote.height;
        let msg_r = sv.vote.round;

        self.cabinet
            .insert(msg_h, msg_r, ctx.clone(), (&sv).into())?;
        let max_w = self
            .cabinet
            .get_pre_commit_max_vote_weight(msg_h, msg_r)
            .expect("Unreachable! pre_commit_max_vote_weight has been set before");
        self.log_vote_received(&sv.voter, (&sv).into(), &max_w);

        self.try_aggregate_pre_commit(ctx, msg_h, msg_r, max_w, sv)
            .await
    }

    async fn pre_handle_signed_choke(
        &mut self,
        ctx: Context,
        sc: SignedChoke,
    ) -> OverlordResult<()> {
        self.filter_msg(sc.choke.height, sc.choke.round, ctx.clone(), (&sc).into())?;
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = auth.verify_signed_choke(&sc).await {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, sc.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, sc.into()));
            }
        });
        Ok(())
    }

    async fn handle_signed_choke(&mut self, ctx: Context, sc: SignedChoke) -> OverlordResult<()> {
        let msg_h = sc.choke.height;
        let msg_r = sc.choke.round;

        self.cabinet
            .insert(msg_h, msg_r, ctx.clone(), (&sc).into())?;
        let sum_w = self
            .cabinet
            .get_choke_vote_weight(msg_h, msg_r)
            .expect("Unreachable! choke_vote_weight has been set before");
        self.log_vote_received(&sc.voter, (&sc).into(), &sum_w);

        self.try_aggregate_choke(ctx.clone(), msg_h, msg_r, sum_w, sc.clone())
            .await?;
        if let Some(from) = sc.from {
            match from {
                UpdateFrom::PreVoteQC(qc) => self.pre_handle_pre_vote_qc(ctx, qc).await?,
                UpdateFrom::PreCommitQC(qc) => self.pre_handle_pre_commit_qc(ctx, qc).await?,
                UpdateFrom::ChokeQC(qc) => self.pre_handle_choke_qc(ctx, qc).await?,
            }
        }
        Ok(())
    }

    async fn pre_handle_pre_vote_qc(&mut self, ctx: Context, qc: PreVoteQC) -> OverlordResult<()> {
        self.filter_msg(qc.vote.height, qc.vote.round, ctx.clone(), (&qc).into())?;
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = auth.verify_pre_vote_qc(&qc).await {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, qc.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, qc.into()));
            }
        });
        Ok(())
    }

    async fn handle_pre_vote_qc(&mut self, ctx: Context, qc: PreVoteQC) -> OverlordResult<()> {
        let msg_h = qc.vote.height;
        let msg_r = qc.vote.round;

        self.cabinet
            .insert(msg_h, msg_r, ctx.clone(), (&qc).into())?;

        if qc.vote.is_empty_vote()
            || self
                .cabinet
                .get_full_block(msg_h, &qc.vote.block_hash)
                .is_some()
        {
            self.handle_pre_vote_qc_with_full_block(ctx, qc).await
        } else {
            Err(OverlordError::warn_wait())
        }
    }

    async fn handle_pre_vote_qc_with_full_block(
        &mut self,
        ctx: Context,
        qc: PreVoteQC,
    ) -> OverlordResult<()> {
        let msg_h = qc.vote.height;
        let msg_r = qc.vote.round;
        let block = self.cabinet.get_block(msg_h, &qc.vote.block_hash);
        let leader = self.auth.get_leader(msg_h, msg_r).await;
        let old_state = self.state.handle_pre_vote_qc(&qc, block)?;
        self.log_state_update_of_msg(old_state, &leader, (&qc).into());
        self.agent
            .set_step_timeout(ctx.clone(), self.state.stage.clone())
            .await;
        self.wal.save_state(&self.state)?;

        let vote = self.state.get_vote();
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        let current_height = self.state.stage.height;
        let current_round = self.state.stage.round;
        tokio::spawn(async move {
            if let Err(e) = create_and_transmit_signed_pre_commit(
                &auth,
                &agent,
                current_height,
                current_round,
                ctx.clone(),
                vote,
            )
            .await
            {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, qc.into(), e));
            }
        });
        Ok(())
    }

    async fn pre_handle_pre_commit_qc(
        &mut self,
        ctx: Context,
        qc: PreCommitQC,
    ) -> OverlordResult<()> {
        self.filter_msg(qc.vote.height, qc.vote.round, ctx.clone(), (&qc).into())?;
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = auth.verify_pre_commit_qc(&qc).await {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, qc.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, qc.into()));
            }
        });
        Ok(())
    }

    async fn handle_pre_commit_qc(&mut self, ctx: Context, qc: PreCommitQC) -> OverlordResult<()> {
        let msg_h = qc.vote.height;
        let msg_r = qc.vote.round;

        self.cabinet
            .insert(msg_h, msg_r, ctx.clone(), (&qc).into())?;

        if qc.vote.is_empty_vote()
            || self
                .cabinet
                .get_full_block(msg_h, &qc.vote.block_hash)
                .is_some()
        {
            self.handle_pre_commit_qc_with_full_block(ctx, qc).await
        } else {
            Err(OverlordError::warn_wait())
        }
    }

    async fn handle_pre_commit_qc_with_full_block(
        &mut self,
        ctx: Context,
        qc: PreCommitQC,
    ) -> OverlordResult<()> {
        let msg_h = qc.vote.height;
        let msg_r = qc.vote.round;
        let block = self.cabinet.get_block(msg_h, &qc.vote.block_hash);
        let leader = self.auth.get_leader(msg_h, msg_r).await;
        let old_state = self.state.handle_pre_commit_qc(&qc, block)?;
        self.log_state_update_of_msg(old_state, &leader, (&qc).into());
        self.wal.save_state(&self.state)?;

        if qc.vote.is_empty_vote() {
            self.new_round(ctx.clone()).await
        } else {
            let (commit_hash, proof, commit_exec_h) = self.save_and_exec_block(ctx.clone()).await;
            self.handle_commit(ctx, commit_hash, proof, commit_exec_h)
                .await
        }
    }

    async fn pre_handle_choke_qc(&mut self, ctx: Context, qc: ChokeQC) -> OverlordResult<()> {
        self.filter_msg(qc.choke.height, qc.choke.round, ctx.clone(), (&qc).into())?;
        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = auth.verify_choke_qc(&qc).await {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, qc.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, qc.into()));
            }
        });
        Ok(())
    }

    async fn handle_choke_qc(&mut self, ctx: Context, qc: ChokeQC) -> OverlordResult<()> {
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
            .handle_commit(commit_exec_result.consensus_config.auth_config.clone())
            .await;
        self.agent
            .handle_commit(commit_exec_result.consensus_config.time_config.clone())
            .await;
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
            && self.next_height_leader().await == self.address
            && self
                .agent
                .set_step_timeout(ctx.clone(), self.state.stage.clone())
                .await
        {
            return Ok(());
        }

        self.next_height(ctx).await
    }

    async fn pre_handle_signed_height(
        &mut self,
        ctx: Context,
        signed_height: SignedHeight,
    ) -> OverlordResult<()> {
        let my_height = self.state.stage.height;
        if signed_height.height <= my_height {
            return Err(OverlordError::debug_old());
        }
        if signed_height.height == my_height + 1 && self.state.stage.step != Step::Brake {
            return Err(OverlordError::debug_not_ready(format!(
                "signed_height.height == my_height + 1 && my.step != Step::Brake, {} != Step::Brake",
                self.state.stage.step
            )));
        }

        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = auth.verify_signed_height(&signed_height) {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, signed_height.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, signed_height.into()));
            }
        });
        Ok(())
    }

    async fn handle_signed_height(
        &mut self,
        ctx: Context,
        signed_height: SignedHeight,
    ) -> OverlordResult<()> {
        let old_sync = self.sync.handle_signed_height(&signed_height)?;
        self.log_sync_update_of_msg(
            old_sync,
            &signed_height.address,
            signed_height.clone().into(),
        );
        self.agent
            .set_sync_timeout(ctx.clone(), self.sync.request_id)
            .await;
        self.agent
            .set_clear_timeout(ctx.clone(), signed_height.address.clone())
            .await;
        let range = HeightRange::new(self.state.stage.height, BLOCK_BATCH);

        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            let request = auth
                .sign_sync_request(range)
                .expect("Unreachable! Should never failed when signing sync request");
            agent
                .transmit(ctx, signed_height.address.clone(), request.into())
                .await
        });
        Ok(())
    }

    async fn pre_handle_sync_request(
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

        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = auth.verify_sync_request(&request) {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, request.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, request.into()));
            }
        });
        Ok(())
    }

    async fn handle_sync_request(
        &mut self,
        ctx: Context,
        request: SyncRequest,
    ) -> OverlordResult<()> {
        let old_sync = self.sync.handle_sync_request(&request)?;
        self.log_sync_update_of_msg(old_sync, &request.requester, request.clone().into());
        self.agent
            .set_clear_timeout(ctx.clone(), request.requester.clone())
            .await;

        let adapter = Arc::clone(&self.adapter);
        let agent = Arc::<_>::clone(&self.agent);
        let auth = Arc::<_>::clone(&self.auth);
        tokio::spawn(async move {
            let block_with_proofs = adapter
                .get_full_block_with_proofs(ctx.clone(), request.request_range.clone())
                .await
                .map_err(OverlordError::local_get_block)?;
            let response = auth
                .sign_sync_response(request.request_range.clone(), block_with_proofs)
                .expect("Unreachable! Should never failed when signing sync response");
            agent
                .transmit(ctx, request.requester, response.into())
                .await
        });
        Ok(())
    }

    async fn pre_handle_sync_response(
        &mut self,
        ctx: Context,
        response: SyncResponse<B, F>,
    ) -> OverlordResult<()> {
        if response.request_range.to < self.state.stage.height {
            return Err(OverlordError::debug_old());
        }

        let auth = Arc::<_>::clone(&self.auth);
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = auth.verify_sync_response(&response) {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, response.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, response.into()));
            }
        });
        Ok(())
    }

    async fn handle_sync_response(
        &mut self,
        ctx: Context,
        response: SyncResponse<B, F>,
    ) -> OverlordResult<()> {
        let map: HashMap<Height, FullBlockWithProof<B, F>> = response
            .block_with_proofs
            .clone()
            .into_iter()
            .map(|fbp| (fbp.full_block.get_block().get_height(), fbp))
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

            let block = &full_block_with_proof.full_block.get_block();
            let proof = &full_block_with_proof.proof;
            let block_hash = block.get_block_hash().map_err(OverlordError::byz_hash)?;
            if proof.vote.block_hash != block_hash {
                return Err(OverlordError::byz_sync(format!(
                    "proof.vote_hash != block.hash, {} != {}",
                    proof.vote.block_hash.tiny_hex(),
                    block_hash.clone().tiny_hex()
                )));
            }
            self.check_block(ctx.clone(), block).await?;
            self.auth.verify_pre_commit_qc(proof).await?;

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
            self.state.stage.set_commit();
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

    async fn pre_handle_fetched_full_block(
        &mut self,
        ctx: Context,
        fetch: FetchedFullBlock<B, F>,
    ) -> OverlordResult<()> {
        if fetch.height < self.state.stage.height {
            return Err(OverlordError::debug_old());
        }

        let wal = self.wal.clone();
        let agent = Arc::<_>::clone(&self.agent);
        tokio::spawn(async move {
            if let Err(e) = wal.save_full_block(&fetch) {
                agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, fetch.into(), e));
            } else {
                agent.send_to_myself(ChannelMsg::HandleMsg(ctx, fetch.into()));
            }
        });
        Ok(())
    }

    async fn handle_fetched_full_block(
        &mut self,
        ctx: Context,
        fetch: FetchedFullBlock<B, F>,
    ) -> OverlordResult<()> {
        self.cabinet.insert_full_block(fetch.clone());
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
            self.handle_pre_commit_qc_with_full_block(ctx, qc).await?;
        } else if let Some(qc) = self.cabinet.get_pre_vote_qc_by_hash(fetch.height, &hash) {
            self.handle_pre_vote_qc_with_full_block(ctx, qc).await?;
        }

        Ok(())
    }

    async fn handle_height_timeout(&mut self, ctx: Context) -> OverlordResult<()> {
        self.agent.set_height_timeout(ctx.clone()).await;

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
        self.agent.next_height().await;
        self.replay_msg_received_before();
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
            .set_step_timeout(ctx.clone(), self.state.stage.clone())
            .await;

        if self.is_current_leader().await {
            let signed_proposal = self.create_signed_proposal(ctx.clone()).await?;
            self.agent.broadcast(ctx, signed_proposal.into()).await?;
        } else if let Some((ctx, signed_proposal)) = self.cabinet.take_signed_proposal(h, r) {
            self.handle_signed_proposal(ctx, signed_proposal).await?;
        }
        Ok(())
    }

    async fn check_block(&self, ctx: Context, block: &B) -> OverlordResult<()> {
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
                ctx,
                block.clone(),
                self.prepare.get_block_states_list(exec_h),
                self.prepare
                    .last_commit_exec_result
                    .block_states
                    .state
                    .clone(),
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
        self.auth.sign_proposal(proposal).await
    }

    fn replay_msg_received_before(&mut self) {
        if self.sync.state == SyncStat::Off {
            if let Some(grids) = self.cabinet.pop(self.state.stage.height) {
                for mut grid in grids {
                    for (ctx, sc) in grid.get_signed_chokes() {
                        self.agent.replay_msg(ctx, sc.into());
                    }
                    if let Some((ctx, qc)) = grid.get_pre_commit_qc() {
                        self.agent.replay_msg(ctx, qc.into());
                    }
                    if let Some((ctx, qc)) = grid.get_pre_vote_qc() {
                        self.agent.replay_msg(ctx, qc.into());
                    }
                    for (ctx, sv) in grid.get_signed_pre_commits() {
                        self.agent.replay_msg(ctx, sv.into());
                    }
                    for (ctx, sv) in grid.get_signed_pre_votes() {
                        self.agent.replay_msg(ctx, sv.into());
                    }
                    if let Some((ctx, sp)) = grid.take_signed_proposal() {
                        self.agent.replay_msg(ctx, sp.into());
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
        sv: SignedPreVote,
    ) -> OverlordResult<()> {
        if self
            .auth
            .current_auth
            .read()
            .await
            .beyond_majority(max_w.cum_weight)
        {
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
            let auth = Arc::<_>::clone(&self.auth);
            let agent = Arc::<_>::clone(&self.agent);
            tokio::spawn(async move {
                if let Err(e) =
                    create_and_broadcast_pre_vote_qc(&auth, &agent, ctx.clone(), votes).await
                {
                    agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, sv.into(), e));
                }
            });
        }
        Ok(())
    }

    async fn try_aggregate_pre_commit(
        &mut self,
        ctx: Context,
        height: Height,
        round: Round,
        max_w: CumWeight,
        sv: SignedPreCommit,
    ) -> OverlordResult<()> {
        if self
            .auth
            .current_auth
            .read()
            .await
            .beyond_majority(max_w.cum_weight)
        {
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
            let auth = Arc::<_>::clone(&self.auth);
            let agent = Arc::<_>::clone(&self.agent);
            tokio::spawn(async move {
                if let Err(e) =
                    create_and_broadcast_pre_commit_qc(&auth, &agent, ctx.clone(), votes).await
                {
                    agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, sv.into(), e));
                }
            });
        }
        Ok(())
    }

    async fn try_aggregate_choke(
        &mut self,
        ctx: Context,
        height: Height,
        round: Round,
        max_w: CumWeight,
        sc: SignedChoke,
    ) -> OverlordResult<()> {
        if self
            .auth
            .current_auth
            .read()
            .await
            .beyond_majority(max_w.cum_weight)
        {
            let chokes = self
                .cabinet
                .get_signed_chokes(height, round)
                .expect("Unreachable! Lost signed_chokes while beyond majority");

            let auth = Arc::<_>::clone(&self.auth);
            let agent = Arc::<_>::clone(&self.agent);
            tokio::spawn(async move {
                if let Err(e) = create_and_send_choke_qc(&auth, &agent, ctx.clone(), chokes).await {
                    agent.send_to_myself(ChannelMsg::HandleMsgError(ctx, sc.into(), e));
                }
            });
        }
        Ok(())
    }

    fn filter_msg(
        &mut self,
        height: Height,
        round: Round,
        ctx: Context,
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
            self.cabinet.insert(height, round, ctx, capsule.clone())?;
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

        self.cabinet.check_exist(height, round, capsule)
    }

    async fn current_leader(&self) -> Address {
        self.auth
            .get_leader(self.state.stage.height, self.state.stage.round)
            .await
    }

    async fn next_height_leader(&self) -> Address {
        self.auth
            .get_leader(self.state.stage.height + 1, INIT_ROUND)
            .await
    }

    async fn is_current_leader(&self) -> bool {
        self.address == self.current_leader().await
    }

    fn extract_sender(&self, msg: &OverlordMsg<B, F>) -> Address {
        match msg {
            OverlordMsg::SignedProposal(sp) => sp.proposal.proposer.clone(),
            OverlordMsg::SignedPreVote(sv) => sv.voter.clone(),
            OverlordMsg::SignedPreCommit(sv) => sv.voter.clone(),
            OverlordMsg::SignedChoke(sc) => sc.voter.clone(),
            OverlordMsg::PreVoteQC(qc) => qc.sender.clone(),
            OverlordMsg::PreCommitQC(qc) => qc.sender.clone(),
            OverlordMsg::ChokeQC(qc) => qc.sender.clone(),
            OverlordMsg::SignedHeight(sh) => sh.address.clone(),
            OverlordMsg::SyncRequest(sq) => sq.requester.clone(),
            OverlordMsg::SyncResponse(sp) => sp.responder.clone(),
            OverlordMsg::FetchedFullBlock(_) => self.address.clone(),
            OverlordMsg::Stop => unreachable!(),
        }
    }

    async fn handle_msg_err(&self, ctx: Context, msg: OverlordMsg<B, F>, e: OverlordError) {
        let sender = self.extract_sender(&msg);
        let content = format!("[RECEIVE]\n\t<{}> <- {}\n\t<message> {}\n\t{}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
                              self.address.tiny_hex(), sender.tiny_hex(), msg, e, self.state, self.prepare, self.sync);
        if let OverlordMsg::FetchedFullBlock(fetch) = msg {
            self.agent.remove_block_hash(fetch.block_hash).await;
        }
        self.handle_err(ctx, e, content).await;
    }

    async fn handle_timeout_err(&self, ctx: Context, timeout: TimeoutEvent, e: OverlordError) {
        let content = format!("[TIMEOUT]\n\t<{}> <- timeout\n\t<timeout> {}\n\t{}\n\t<state> {}\n\t<prepare> {}\n\t<sync> {}\n",
                              self.address.tiny_hex(), timeout, e, self.state, self.prepare, self.sync);
        self.handle_err(ctx, e, content).await;
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

    fn log_sync_update_of_msg(&self, old_sync: Sync<B>, from: &Address, msg: OverlordMsg<B, F>) {
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

async fn pre_handle_signed_proposal<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    state_round: Round,
    prepare: ProposePrepare<S>,
    auth: Arc<AuthManage<A, B, F, S>>,
    agent: &Arc<EventAgent<A, B, F, S>>,
    adapter: Arc<A>,
    ctx: Context,
    sp: SignedProposal<B>,
) -> OverlordResult<OverlordMsg<B, F>> {
    check_proposal(prepare.pre_hash.clone(), &auth, &sp.proposal).await?;
    auth.verify_signed_proposal(&sp).await?;
    check_block(prepare, &adapter, ctx.clone(), &sp.proposal.block).await?;

    agent
        .request_full_block(ctx.clone(), sp.proposal.block.clone())
        .await;
    if sp.proposal.round > state_round {
        if let Some(qc) = sp.proposal.lock {
            agent.send_to_myself(ChannelMsg::HandleMsg(ctx, qc.into()));
        }
        return Err(OverlordError::debug_high());
    }
    Ok(sp.into())
}

async fn create_and_transmit_signed_pre_vote<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    auth: &Arc<AuthManage<A, B, F, S>>,
    agent: &Arc<EventAgent<A, B, F, S>>,
    current_height: Height,
    current_round: Round,
    ctx: Context,
    vote: Vote,
) -> OverlordResult<()> {
    let signed_pre_vote = auth.sign_pre_vote(vote).await?;
    transmit_vote(
        auth,
        agent,
        current_height,
        current_round,
        ctx,
        signed_pre_vote.into(),
    )
    .await
}

async fn create_and_transmit_signed_pre_commit<
    A: Adapter<B, F, S>,
    B: Blk,
    F: FullBlk<B>,
    S: St,
>(
    auth: &Arc<AuthManage<A, B, F, S>>,
    agent: &Arc<EventAgent<A, B, F, S>>,
    current_height: Height,
    current_round: Round,
    ctx: Context,
    vote: Vote,
) -> OverlordResult<()> {
    let signed_pre_commit = auth.sign_pre_commit(vote).await?;
    transmit_vote(
        auth,
        agent,
        current_height,
        current_round,
        ctx,
        signed_pre_commit.into(),
    )
    .await
}

async fn create_and_broadcast_pre_vote_qc<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    auth: &Arc<AuthManage<A, B, F, S>>,
    agent: &Arc<EventAgent<A, B, F, S>>,
    ctx: Context,
    pre_votes: Vec<SignedPreVote>,
) -> OverlordResult<()> {
    let pre_vote_qc = auth.aggregate_pre_votes(pre_votes).await?;
    agent.broadcast(ctx, pre_vote_qc.into()).await
}

async fn create_and_broadcast_pre_commit_qc<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    auth: &Arc<AuthManage<A, B, F, S>>,
    agent: &Arc<EventAgent<A, B, F, S>>,
    ctx: Context,
    pre_commits: Vec<SignedPreCommit>,
) -> OverlordResult<()> {
    let pre_commit_qc = auth.aggregate_pre_commits(pre_commits).await?;
    agent.broadcast(ctx, pre_commit_qc.into()).await
}

async fn create_and_send_choke_qc<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    auth: &Arc<AuthManage<A, B, F, S>>,
    agent: &Arc<EventAgent<A, B, F, S>>,
    ctx: Context,
    chokes: Vec<SignedChoke>,
) -> OverlordResult<()> {
    let choke_qc = auth.aggregate_chokes(chokes).await?;
    agent.send_to_myself(ChannelMsg::HandleMsg(ctx, choke_qc.into()));
    Ok(())
}

async fn create_and_broadcast_signed_choke<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    auth: &Arc<AuthManage<A, B, F, S>>,
    agent: &Arc<EventAgent<A, B, F, S>>,
    ctx: Context,
    choke: Choke,
    from: Option<UpdateFrom>,
) -> OverlordResult<()> {
    let signed_choke = auth.sign_choke(choke, from).await?;
    agent.broadcast(ctx, signed_choke.into()).await
}

async fn transmit_vote<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    auth: &Arc<AuthManage<A, B, F, S>>,
    agent: &Arc<EventAgent<A, B, F, S>>,
    current_height: Height,
    current_round: Round,
    ctx: Context,
    vote: OverlordMsg<B, F>,
) -> OverlordResult<()> {
    let leader = current_leader(auth, current_height, current_round).await;
    agent.transmit(ctx.clone(), leader, vote.clone()).await?;
    // new design, when leader is down, next leader can aggregate votes to view change fast
    let next_leader = next_round_leader(auth, current_height, current_round).await;
    agent.transmit(ctx, next_leader, vote).await
}

async fn check_proposal<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    pre_hash: Hash,
    auth: &Arc<AuthManage<A, B, F, S>>,
    p: &Proposal<B>,
) -> OverlordResult<()> {
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

    if pre_hash != p.block.get_pre_hash() {
        return Err(OverlordError::byz_block(format!(
            "self.pre_hash != block.pre_hash, {} != {}",
            pre_hash.tiny_hex(),
            p.block.get_pre_hash().tiny_hex()
        )));
    }

    auth.verify_proof(p.block.get_proof()).await?;

    if let Some(lock) = &p.lock {
        if lock.vote.is_empty_vote() {
            return Err(OverlordError::byz_empty_lock());
        }
        auth.verify_pre_vote_qc(&lock).await?;
    }
    Ok(())
}

async fn check_block<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    prepare: ProposePrepare<S>,
    adapter: &Arc<A>,
    ctx: Context,
    block: &B,
) -> OverlordResult<()> {
    let exec_h = block.get_exec_height();
    if block.get_height() > exec_h + prepare.max_exec_behind {
        return Err(OverlordError::byz_block(format!(
            "block.block_height > block.exec_height + max_exec_behind, {} > {} + {}",
            block.get_height(),
            exec_h,
            prepare.max_exec_behind
        )));
    }
    if prepare.exec_height < exec_h {
        return Err(OverlordError::warn_block(format!(
            "self.exec_height < block.exec_height, {} < {}",
            prepare.exec_height, exec_h
        )));
    }

    adapter
        .check_block(
            ctx,
            block.clone(),
            prepare.get_block_states_list(exec_h),
            prepare.last_commit_exec_result.block_states.state.clone(),
        )
        .await
        .map_err(OverlordError::byz_adapter_check_block)?;
    Ok(())
}

async fn current_leader<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    auth: &Arc<AuthManage<A, B, F, S>>,
    current_height: Height,
    current_round: Round,
) -> Address {
    auth.get_leader(current_height, current_round).await
}

async fn next_round_leader<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    auth: &Arc<AuthManage<A, B, F, S>>,
    current_height: Height,
    current_round: Round,
) -> Address {
    auth.get_leader(current_height, current_round).await
}

// async fn next_height_leader<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
//     auth: &Arc<AuthManage<A, B, F, S>>,
//     current_height: Height,
// ) -> Address {
//     auth.get_leader(current_height + 1, INIT_ROUND).await
// }

// async fn is_current_leader<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
//     auth: &Arc<AuthManage<A, B, F, S>>,
//     current_height: Height,
//     current_round: Round,
//     address: Address,
// ) -> bool {
//     address == current_leader(auth, current_height, current_round).await
// }

async fn recover_state_by_adapter<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    adapter: &Arc<A>,
    ctx: Context,
) -> StateInfo<B> {
    let height = adapter.get_latest_height(ctx).await.expect(
        "Cannot get the latest height from the adapter! It's meaningless to continue running",
    );
    StateInfo::from_commit_height(height)
}

async fn recover_propose_prepare_and_config<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
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
    let block = full_block_with_proof.full_block.get_block();
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

async fn get_full_block_with_proofs<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
    adapter: &Arc<A>,
    ctx: Context,
    height: Height,
) -> OverlordResult<Option<FullBlockWithProof<B, F>>> {
    let vec = adapter
        .get_full_block_with_proofs(ctx, HeightRange::new(height, 1))
        .await
        .map_err(OverlordError::local_get_block)?;
    Ok(Some(vec[0].clone()))
}

async fn get_exec_result<A: Adapter<B, F, S>, B: Blk, F: FullBlk<B>, S: St>(
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
