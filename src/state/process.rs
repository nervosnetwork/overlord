use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use std::{ops::BitXor, sync::Arc};

use bit_vec::BitVec;
use bytes::Bytes;
use futures::{channel::mpsc::UnboundedReceiver, select, StreamExt};
use futures_timer::Delay;
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

/// **TODO: context libiary**
const CTX: u8 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
enum MsgType {
    SignedProposal,
    SignedVote,
}

/// Overlord state struct. It maintains the local state of the node, and monitor the SMR event. The
/// `proposals` is used to cache the signed proposals that are with higher epoch ID or round. The
/// `hash_with_epoch` field saves hash and its corresponding epoch with the current epoch ID and
/// round. The `votes` field saves all signed votes and quorum certificates which epoch ID is higher
/// than `current_epoch - 1`.
pub struct State<T: Codec, F: Consensus<T>, C: Crypto> {
    epoch_id:             u64,
    round:                u64,
    state_machine:        SMR,
    address:              Address,
    proposals:            ProposalCollector<T>,
    votes:                VoteCollector,
    authority:            AuthorityManage,
    hash_with_epoch:      HashMap<Hash, T>,
    full_transcation:     Arc<Mutex<HashSet<Hash>>>,
    is_leader:            bool,
    leader_address:       Address,
    last_commit_round:    Option<u64>,
    last_commit_proposal: Option<Hash>,
    epoch_start:          Instant,
    epoch_interval:       u64,

    function: Arc<F>,
    util:     C,
}

impl<T, F, C> State<T, F, C>
where
    T: Codec,
    F: Consensus<T> + 'static,
    C: Crypto,
{
    ///
    pub fn new(smr: SMR, addr: Address, interval: u64, consensus: F, crypto: C) -> Self {
        State {
            epoch_id:             INIT_EPOCH_ID,
            round:                INIT_ROUND,
            state_machine:        smr,
            address:              addr,
            proposals:            ProposalCollector::new(),
            votes:                VoteCollector::new(),
            authority:            AuthorityManage::new(),
            hash_with_epoch:      HashMap::new(),
            full_transcation:     Arc::new(Mutex::new(HashSet::new())),
            is_leader:            false,
            leader_address:       Address::default(),
            last_commit_round:    None,
            last_commit_proposal: None,
            epoch_start:          Instant::now(),
            epoch_interval:       interval,

            function: Arc::new(consensus),
            util:     crypto,
        }
    }

    ///
    pub async fn run(
        &mut self,
        mut rx: UnboundedReceiver<OverlordMsg<T>>,
        mut event: Event,
    ) -> ConsensusResult<()> {
        loop {
            select! {
                raw = rx.next() => self.handle_msg(raw).await?,
                evt = event.next() => self.handle_event(evt).await?,
            }
        }
    }

    async fn handle_msg(&mut self, msg: Option<OverlordMsg<T>>) -> ConsensusResult<()> {
        match msg.ok_or_else(|| ConsensusError::Other("Message sender dropped".to_string()))? {
            OverlordMsg::SignedProposal(sp) => self.handle_signed_proposal(sp).await,
            OverlordMsg::AggregatedVote(av) => self.handle_aggregated_vote(av).await,
            OverlordMsg::SignedVote(sv) => self.handle_signed_vote(sv).await,
        }
    }

    async fn handle_event(&mut self, event: Option<SMREvent>) -> ConsensusResult<()> {
        match event.ok_or_else(|| ConsensusError::Other("Event sender dropped".to_string()))? {
            SMREvent::NewRoundInfo {
                round,
                lock_round,
                lock_proposal,
            } => {
                self.handle_new_round(round, lock_round, lock_proposal)
                    .await
            }
            SMREvent::PrevoteVote(hash) => self.handle_prevote_vote(hash).await,
            SMREvent::PrecommitVote(hash) => self.handle_precommit_vote(hash).await,
            SMREvent::Commit(hash) => self.handle_commit(hash).await,
            _ => unreachable!(),
        }
    }

    async fn goto_new_epoch(&mut self, status: Status) -> ConsensusResult<()> {
        let new_epoch_id = status.epoch_id;
        if new_epoch_id != self.epoch_id + 1 {
            let mut tmp = self
                .function
                .get_authority_list(vec![CTX], new_epoch_id - 1)
                .await
                .map_err(|err| ConsensusError::Other(format!("{:?}", err)))?;
            self.authority.update(&mut tmp, false);
        }

        self.epoch_id = new_epoch_id;
        self.round = INIT_ROUND;
        let mut auth_list = status.authority_list;
        self.authority.update(&mut auth_list, true);
        self.epoch_interval = status.interval;

        // TODO: recheck proposals and votes.
        // TODO: clear outdate proposals and votes.

        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::NewEpoch(new_epoch_id),
            source:       TriggerSource::State,
            hash:         Hash::new(),
            round:        None,
        })?;
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
        self.round = round;
        self.is_leader = false;

        if lock_round.is_some().bitxor(lock_proposal.is_some()) {
            return Err(ConsensusError::ProposalErr(
                "Lock round is inconsistent with lock proposal".to_string(),
            ));
        }

        if lock_proposal.is_none() {
            // Clear full transcation signal.
            self.full_transcation = Arc::new(Mutex::new(HashSet::new()));
            // Clear hash with epoch.
            self.hash_with_epoch.clear();
        }

        // If self is not proposer, check whether it has received current signed proposal before. If
        // has, then handle it.
        if !self.is_proposer()? {
            if let Ok(signed_proposal) = self.proposals.get(self.epoch_id, self.round) {
                return self.handle_signed_proposal(signed_proposal).await;
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
        self.broadcast(OverlordMsg::SignedProposal(self.sign_proposal(proposal)?))
            .await?;

        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::Proposal,
            source:       TriggerSource::State,
            hash:         hash.clone(),
            round:        lock_round,
        })?;

        let epoch_id = self.epoch_id;
        let tx_signal = Arc::clone(&self.full_transcation);
        let function = Arc::clone(&self.function);
        tokio::spawn(async move {
            let _ = check_current_epoch(function, tx_signal, epoch_id, hash).await;
        });

        Ok(())
    }

    /// This function only handle signed proposals which epoch ID and round are equal to current.
    /// Others will be ignored or storaged in the proposal collector.
    async fn handle_signed_proposal(
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
        // equal to the current epoch ID and the proposal round is higher than the current round,
        // cache it until that epoch ID.
        if epoch_id > self.epoch_id || (epoch_id == self.epoch_id && round > self.round) {
            self.proposals.insert(epoch_id, round, signed_proposal)?;
            return Ok(());
        }

        //  Verify proposal signature.
        let proposal = signed_proposal.proposal;
        let signature = signed_proposal.signature;
        self.verify_address(&proposal.proposer, epoch_id == self.epoch_id)?;
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
                    return Ok(());
                }

                self.retransmit_vote(
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

        let hash = proposal.epoch_hash.clone();
        self.hash_with_epoch.insert(hash.clone(), proposal.content);
        self.state_machine.trigger(SMRTrigger {
            trigger_type: TriggerType::Proposal,
            source:       TriggerSource::State,
            hash:         hash.clone(),
            round:        lock_round,
        })?;

        let epoch_id = self.epoch_id;
        let tx_signal = Arc::clone(&self.full_transcation);
        let function = Arc::clone(&self.function);
        tokio::spawn(async move {
            let _ = check_current_epoch(function, tx_signal, epoch_id, hash).await;
        });
        Ok(())
    }

    async fn handle_prevote_vote(&mut self, hash: Hash) -> ConsensusResult<()> {
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
            self.transmit(OverlordMsg::SignedVote(signed_vote)).await?;
        }
        Ok(())
    }

    async fn handle_precommit_vote(&mut self, hash: Hash) -> ConsensusResult<()> {
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
            self.transmit(OverlordMsg::SignedVote(signed_vote)).await?;
        }
        Ok(())
    }

    async fn handle_commit(&mut self, hash: Hash) -> ConsensusResult<()> {
        let epoch = self.epoch_id;
        let content = self
            .hash_with_epoch
            .get(&hash)
            .ok_or_else(|| {
                ConsensusError::Other(format!(
                    "Lose the whole epoch epoch ID {}, round {}",
                    self.epoch_id, self.round
                ))
            })?
            .to_owned();
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

        let status = self
            .function
            .commit(vec![CTX], epoch, commit)
            .await
            .map_err(|err| ConsensusError::Other(format!("{:?}", err)))?;

        if Instant::now() < self.epoch_start + Duration::from_millis(self.epoch_interval) {
            Delay::new_at(self.epoch_start + Duration::from_millis(self.epoch_interval))
                .await
                .map_err(|err| ConsensusError::Other(format!("Overlord delay error {:?}", err)))?;
        }

        self.goto_new_epoch(status).await?;
        Ok(())
    }

    /// The main process of handle signed vote is that only handle those epoch ID and round are both
    /// equal to the current. The lower votes will be ignored directly even if the epoch ID is equal
    /// to the `current epoch ID - 1` and the round is higher than the current round. The reason is
    /// that the effective leader must in the lower epoch, and the task of handling signed votes
    /// will be done by the leader. For the higher votes, check the signature and save them in
    /// the vote collector. Whenevet the current vote is received, a statistic is made to check
    /// if the sum of the voting weights corresponding to the hash exceeds the threshold.
    async fn handle_signed_vote(&mut self, signed_vote: SignedVote) -> ConsensusResult<()> {
        let epoch_id = signed_vote.get_epoch();
        let round = signed_vote.get_round();
        let vote_type = if signed_vote.is_prevote() {
            VoteType::Prevote
        } else {
            VoteType::Precommit
        };

        // If the vote epoch ID is lower than the current epoch ID - 1, or the vote epoch ID
        // is equal to the current epoch ID and the vote round is lower than the current round,
        // ignore it directly.
        if epoch_id < self.epoch_id || (epoch_id == self.epoch_id && round < self.round) {
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
            return Ok(());
        }

        if !self.is_leader {
            // return Ok or Err
            return Ok(());
        }

        // Check whether there is a hash that vote weight is above the threshold. If no hash
        // achieved this, return directly.
        let vote_map = self
            .votes
            .get_vote_map(epoch_id, round, vote_type.clone())?;
        let mut epoch_hash = None;
        let threshold = self.authority.get_vote_weight_sum(true)? * 2;
        for (hash, set) in vote_map.iter() {
            let mut acc = 0u8;
            for addr in set.iter() {
                acc += self.authority.get_vote_weight(addr)?;
            }
            if u64::from(acc) * 3 > threshold {
                epoch_hash = Some(hash.to_owned());
                break;
            }
        }
        if epoch_hash.is_none() {
            return Ok(());
        }

        // Build the quorum certificate needs to aggregate signatures into an aggregate
        // signature besides the address bitmap.
        let epoch_hash = epoch_hash.unwrap();
        let votes = self
            .votes
            .get_votes(epoch_id, round, vote_type.clone(), &epoch_hash)?;
        let mut signatures = Vec::new();
        let mut set = Vec::new();
        for vote in votes.iter() {
            signatures.push(vote.signature.clone());
            set.push(vote.vote.voter.clone());
        }
        let set = set.iter().cloned().collect::<HashSet<Address>>();
        let mut bit_map = BitVec::from_elem(self.authority.current_len(), false);
        for (index, addr) in self.authority.get_addres_ref().iter().enumerate() {
            if set.contains(addr) {
                bit_map.set(index, true);
            }
        }
        let aggregated_signature = AggregatedSignature {
            signature:      self.aggregate_signatures(signatures)?,
            address_bitmap: Bytes::from(bit_map.to_bytes()),
        };
        let qc = AggregatedVote {
            signature:  aggregated_signature,
            vote_type:  vote_type.clone(),
            epoch_id:   self.epoch_id,
            round:      self.round,
            epoch_hash: epoch_hash.clone(),
            leader:     self.address.clone(),
        };

        self.votes.set_qc(qc.clone());
        self.broadcast(OverlordMsg::AggregatedVote(qc)).await?;

        let set = self.full_transcation.lock();
        let epoch_hash = if set.contains(&epoch_hash) {
            epoch_hash
        } else {
            Hash::new()
        };
        self.state_machine.trigger(SMRTrigger {
            trigger_type: vote_type.into(),
            source:       TriggerSource::State,
            hash:         epoch_hash,
            round:        Some(round),
        })?;

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
        aggregated_vote: AggregatedVote,
    ) -> ConsensusResult<()> {
        let epoch_id = aggregated_vote.get_epoch();
        let round = aggregated_vote.get_round();
        let qc_type = if aggregated_vote.is_prevote_qc() {
            VoteType::Prevote
        } else {
            VoteType::Precommit
        };

        // If the vote epoch ID is lower than the current epoch ID - 1, or the vote epoch ID
        // is equal to the current epoch ID and the vote round is lower than the current round,
        // ignore it directly.
        if epoch_id < self.epoch_id - 1 || (epoch_id == self.epoch_id && round < self.round) {
            return Ok(());
        } else if epoch_id > self.epoch_id {
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
            self.votes.set_qc(aggregated_vote);
            return Ok(());
        }

        // Deal with QC's epoch ID is equal to the current epoch ID - 1 and round is higher than the
        // last commit round. If the QC is a prevoteQC, ignore it. Retransmit precommit vote to the
        // last commit proposal.
        if epoch_id == self.epoch_id - 1 {
            if let Some((last_round, last_proposal)) = self.last_commit_msg()? {
                if round <= last_round || qc_type == VoteType::Precommit {
                    return Ok(());
                }

                self.retransmit_vote(
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

        let set = self.full_transcation.lock();
        let epoch_hash = if set.contains(&qc_hash) {
            qc_hash
        } else {
            Hash::new()
        };
        self.state_machine.trigger(SMRTrigger {
            trigger_type: qc_type.into(),
            source:       TriggerSource::State,
            hash:         epoch_hash,
            round:        Some(round),
        })?;
        Ok(())
    }

    /// Check last commit round and last commit proposal.
    fn last_commit_msg(&self) -> ConsensusResult<Option<(u64, Hash)>> {
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
        let signature = self
            .util
            .sign(self.util.hash(Bytes::from(encode(&vote))))
            .map_err(|err| ConsensusError::CryptoErr(format!("{:?}", err)))?;
        Ok(SignedVote { signature, vote })
    }

    fn aggregate_signatures(&self, signatures: Vec<Signature>) -> ConsensusResult<Signature> {
        let signature = self
            .util
            .aggregate_signatures(signatures)
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

    /// Check whether the given address is included in the corresponding authority list.
    fn verify_address(&self, address: &Address, is_current: bool) -> ConsensusResult<()> {
        if !self.authority.contains(address, is_current)? {
            return Err(ConsensusError::InvalidAddress);
        }
        Ok(())
    }

    async fn transmit(&self, msg: OverlordMsg<T>) -> ConsensusResult<()> {
        self.function
            .transmit_to_relayer(vec![CTX], self.leader_address.clone(), msg)
            .await
            .map_err(|err| ConsensusError::Other(format!("{:?}", err)))?;
        Ok(())
    }

    async fn retransmit_vote(
        &self,
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

        self.function
            .transmit_to_relayer(
                vec![CTX],
                leader_address,
                OverlordMsg::SignedVote(self.sign_vote(vote)?),
            )
            .await
            .map_err(|err| ConsensusError::Other(format!("{:?}", err)))?;
        Ok(())
    }

    async fn broadcast(&self, msg: OverlordMsg<T>) -> ConsensusResult<()> {
        self.function
            .broadcast_to_other(vec![CTX], msg)
            .await
            .map_err(|err| ConsensusError::Other(format!("{:?}", err)))?;
        Ok(())
    }
}

async fn check_current_epoch<U: Consensus<S>, S: Codec>(
    function: Arc<U>,
    tx_signal: Arc<Mutex<HashSet<Hash>>>,
    epoch_id: u64,
    hash: Hash,
) -> ConsensusResult<()> {
    let _transcation = function
        .check_epoch(vec![CTX], epoch_id, hash.clone())
        .await
        .map_err(|err| ConsensusError::Other(format!("{:?}", err)))?;
    let mut set = tx_signal.lock();
    set.insert(hash);
    // TODO: write Wal
    Ok(())
}

#[cfg(test)]
mod test {
    use bit_vec::BitVec;
    use bytes::Bytes;
    use rand::random;

    #[test]
    fn test_bitmap() {
        let len = random::<u8>() as usize;
        let bitmap = (0..len).map(|_| random::<bool>()).collect::<Vec<_>>();
        let mut bv = BitVec::from_elem(len, false);
        for (index, is_vote) in bitmap.iter().enumerate() {
            if *is_vote {
                bv.set(index, true);
            }
        }

        let tmp = Bytes::from(bv.to_bytes());
        let output = BitVec::from_bytes(tmp.as_ref());

        for item in output.iter().zip(bitmap.iter()) {
            assert_eq!(item.0, *item.1);
        }
    }
}
